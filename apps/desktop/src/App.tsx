import { useCallback, useEffect, useMemo, useRef, useState } from 'react';
import { invoke } from '@tauri-apps/api/core';
import { useVirtualizer } from '@tanstack/react-virtual';
import { ArrowDownToLine, ArrowRight, Check, CheckCircle2, ChevronRight, Disc3, File, FileImage, FileSearch, FolderOpen, HardDrive, History, House, Info, LoaderCircle, LockKeyhole, Moon, RefreshCw, Search, Settings2, ShieldCheck, SlidersHorizontal, Sun, X, AlertTriangle } from 'lucide-react';
import type { Entry, Job, Lang, Preview, Section, Session, Source } from './types';
import { appendProgressSample } from './progress';
import { JobProgress } from './JobProgress';
import type { ProgressSample } from './progress';
import { filterEntries, formatBytes, parseRange, formatSessionDate, safeImageUrl, selectedVisible, toggleVisible } from './model';

const native = '__TAURI_INTERNALS__' in window;
const initialJob = (id: string, mode: string, sourceName: string): Job => ({ id, mode, sourceName, stage: 'preparing', elapsedSeconds: 0, progressDeterminate: false, sleepInhibited: false, sleepInhibitionError: null, status: 'running', progress: { scanned: 0, total: 0, found: 0 }, entries: [], warnings: [], error: null });
const readSetting = (key: string, fallback: string) => { try { return localStorage.getItem(key) || fallback; } catch { return fallback; } };
const writeSetting = (key: string, value: string) => { try { localStorage.setItem(key, value); } catch { /* App settings may be unavailable in a restricted profile. */ } };

export default function App() {
  const [lang, setLang] = useState<Lang>(readSetting('mr-language', 'ru') === 'en' ? 'en' : 'ru');
  const [theme, setTheme] = useState(readSetting('mr-theme', 'system'));
  const [section, setSection] = useState<Section>('home');
  const [sources, setSources] = useState<Source[]>([]);
  const [sourceId, setSourceId] = useState('');
  const [sessions, setSessions] = useState<Session[]>([]);
  const [error, setError] = useState('');
  const [notice, setNotice] = useState('');
  const [pending, setPending] = useState(false);
  const [mode, setMode] = useState('quick');
  const [rangeStart, setRangeStart] = useState('');
  const [rangeEnd, setRangeEnd] = useState('');
  const [types, setTypes] = useState<string[]>([]);
  const [scan, setScan] = useState<Job | null>(null);
  const [scanSourceName, setScanSourceName] = useState('');
  const [task, setTask] = useState<Job | null>(null);
  const [active, setActive] = useState<{ id: string; kind: 'scan' | 'recovery' | 'image' } | null>(null);
  const [startedAt, setStartedAt] = useState(0);
  const [now, setNow] = useState(Date.now());
  const [launchKind, setLaunchKind] = useState<'scan' | 'recovery' | 'image' | null>(null);
  const [cancelling, setCancelling] = useState(false);
  const [cancelError, setCancelError] = useState('');
  const launchCancel = useRef(false);
  const [pollError, setPollError] = useState('');
  const [pollRetry, setPollRetry] = useState(0);
  const [lastResponseAt, setLastResponseAt] = useState(0);
  const [progressSamples, setProgressSamples] = useState<ProgressSample[]>([]);
  const scanProgressAnchor = useRef<HTMLDivElement>(null);
  const [selected, setSelected] = useState<Set<string>>(new Set());
  const [focused, setFocused] = useState<Entry | null>(null);
  const [preview, setPreview] = useState<Preview | null>(null);
  const [previewError, setPreviewError] = useState('');
  const [previewTab, setPreviewTab] = useState('preview');
  const [query, setQuery] = useState('');
  const [kind, setKind] = useState('');
  const [sort, setSort] = useState('name');
  const [destination, setDestination] = useState('');
  const [imageDestination, setImageDestination] = useState('');
  const [showRecovery, setShowRecovery] = useState(false);
  const recoveryDialog = useRef<HTMLDialogElement>(null);
  const t = (ru: string, en: string) => lang === 'ru' ? ru : en;
  const source = sources.find(s => s.id === sourceId);
  const busy = pending || !!active || !!launchKind;
  const elapsed = Math.max(0, (now - startedAt) / 1000);

  useEffect(() => {
    document.documentElement.lang = lang;
    writeSetting('mr-language', lang);
  }, [lang]);
  useEffect(() => {
    writeSetting('mr-theme', theme);
    const mq = window.matchMedia('(prefers-color-scheme: dark)');
    const apply = () => document.documentElement.dataset.theme = theme === 'system' ? mq.matches ? 'dark' : 'light' : theme;
    apply(); mq.addEventListener('change', apply);
    return () => mq.removeEventListener('change', apply);
  }, [theme]);
  const refreshSources = useCallback(async () => {
    const result = await invoke<Source[]>('list_sources');
    setSources(previous => [...result, ...previous.filter(s => s.kind === 'image' && !result.some(r => r.id === s.id))]);
  }, []);
  const refreshSessions = useCallback(async () => { setSessions(await invoke<Session[]>('list_sessions')); }, []);
  useEffect(() => {
    if (!native) return;
    Promise.all([refreshSources(), refreshSessions()]).catch(e => setError(String(e)));
  }, [refreshSources, refreshSessions]);
  useEffect(() => {
    if (!active) return;
    let stopped = false; let timer: ReturnType<typeof setTimeout>;
    const poll = async () => {
      try {
        const result = await invoke<Job>('job_status', { jobId: active.id });
        if (stopped) return;
        const receivedAt = Date.now();
        setLastResponseAt(receivedAt); setPollError('');
        setProgressSamples(previous => appendProgressSample(previous, { timeMs: receivedAt, stage: result.stage, scanned: result.progress.scanned, total: result.progress.total, bytesRead: result.progress.bytes_read }));
        if (active.kind === 'scan') {
          setScan(result);
          if (result.sourceId) setSourceId(result.sourceId);
          if (['quick', 'deep', 'advanced'].includes(result.mode)) setMode(result.mode);
        } else setTask(result);
        if (result.status !== 'running') { setActive(null); setCancelling(false); if (result.error && result.status === 'failed') setError(result.error); return; }
        timer = setTimeout(poll, 500);
      } catch (e) { if (!stopped) { setPollError(String(e)); timer = setTimeout(poll, 2000); } }
    };
    void poll();
    return () => { stopped = true; clearTimeout(timer); };
  }, [active, pollRetry]);
  useEffect(() => {
    if (!active && !launchKind) return;
    setNow(Date.now());
    const timer = setInterval(() => setNow(Date.now()), 1000);
    return () => clearInterval(timer);
  }, [active, launchKind]);
  useEffect(() => {
    if (launchKind === 'scan') scanProgressAnchor.current?.scrollIntoView({ block: 'nearest' });
  }, [launchKind]);
  useEffect(() => {
    setPreview(null); setPreviewError('');
    if (!focused || !scan) return;
    let stopped = false;
    invoke<Preview>('preview', { jobId: scan.id, entryId: focused.id }).then(result => { if (!stopped) setPreview(result); }).catch(e => { if (!stopped) setPreviewError(String(e)); });
    return () => { stopped = true; };
  }, [focused, scan?.id]);
  useEffect(() => {
    const dialog = recoveryDialog.current;
    if (showRecovery && dialog && !dialog.open) dialog.showModal();
    if (!showRecovery && dialog?.open) dialog.close();
  }, [showRecovery]);

  async function perform(action: () => Promise<void>) {
    setError(''); setNotice(''); setPending(true);
    try { await action(); } catch (e) { setError(String(e)); } finally { setPending(false); }
  }
  function selectSource(id: string) {
    setSourceId(id); setScan(null); setTask(null); setSelected(new Set()); setFocused(null); setScanSourceName('');
  }
  async function pickSource() {
    const picked = await invoke<Source | null>('pick_source');
    if (picked) { setSources(previous => [...previous.filter(s => s.id !== picked.id), picked]); selectSource(picked.id); if (section !== 'images') setSection('recover'); }
  }
  async function requestCancellation() {
    setCancelling(true); setCancelError('');
    if (!active) { launchCancel.current = true; return; }
    try { await invoke('cancel_job', { jobId: active.id }); }
    catch (e) { setCancelError(String(e)); setCancelling(false); }
  }
  async function launchJob(jobKind: 'scan' | 'recovery' | 'image', jobMode: string, sourceName: string, start: () => Promise<string>) {
    const startTime = Date.now();
    setStartedAt(startTime); setNow(startTime); setLaunchKind(jobKind);
    setCancelling(false); setCancelError(''); launchCancel.current = false;
    setPollError(''); setLastResponseAt(0); setProgressSamples([]);
    const preparing = initialJob('', jobMode, sourceName);
    if (jobKind === 'scan') setScan(preparing); else setTask(preparing);
    try {
      const id = await start();
      setActive({ id, kind: jobKind }); setLaunchKind(null);
      if (jobKind === 'scan') setScan({ ...preparing, id }); else setTask({ ...preparing, id });
      if (launchCancel.current) {
        try { await invoke('cancel_job', { jobId: id }); }
        catch (e) { setCancelError(String(e)); setCancelling(false); }
      }
      return id;
    } catch (e) {
      const failed: Job = { ...preparing, status: 'failed', stage: 'finished', elapsedSeconds: (Date.now() - startTime) / 1000, error: String(e) };
      if (jobKind === 'scan') setScan(failed); else setTask(failed);
      setLaunchKind(null); setCancelling(false);
      throw e;
    }
  }
  async function startScan() {
    if (!source) return;
    let range = { start: null, end: null } as { start: number | null; end: number | null };
    if (mode === 'advanced') {
      try { range = parseRange(rangeStart, rangeEnd, source.size); }
      catch { throw new Error(t('Укажите целые байтовые смещения: начало меньше конца, диапазон внутри источника.', 'Use whole byte offsets: start must precede end and the range must fit within the source.')); }
    }
    setSelected(new Set()); setFocused(null); setQuery(''); setKind(''); setTask(null);
    setScanSourceName(source.name);
    await launchJob('scan', mode, source.name, () => invoke<string>('start_scan', { sourceId, mode, ...range, types: mode === 'quick' ? [] : types }));
  }
  async function exportManifest() {
    if (!scan) return;
    const saved = await invoke<string | null>('save_manifest', { jobId: scan.id });
    if (saved) { setNotice(`${t('Манифест сохранён', 'Manifest saved')}: ${saved}`); await refreshSessions(); }
  }
  const entries = scan?.entries ?? [];
  const filtered = useMemo(() => filterEntries(entries, query, kind, sort), [entries, query, kind, sort]);
  const kinds = useMemo(() => [...new Set(entries.map(e => e.kind))].sort(), [entries]);
  const selectedBytes = entries.reduce((sum, e) => sum + (selected.has(e.id) ? e.size : 0), 0);
  const runningKind = active?.kind || launchKind;
  const running = runningKind === 'scan' ? scan : task;
  const scanActive = runningKind === 'scan';
  const taskActive = !!runningKind && runningKind !== 'scan';
  const liveMonitor = { now, elapsed, lastResponseAt, pollError, progressSamples, cancelling, cancelError, launching: !!launchKind, onRetry: () => setPollRetry(value => value + 1) };
  const nav = [
    { id: 'home' as const, icon: House, title: t('Главная', 'Home') },
    { id: 'recover' as const, icon: FileSearch, title: t('Восстановление', 'Recover files') },
    { id: 'images' as const, icon: Disc3, title: t('Образы дисков', 'Disk images') },
    { id: 'sessions' as const, icon: History, title: t('Сохранённые сессии', 'Saved sessions') },
    { id: 'advanced' as const, icon: SlidersHorizontal, title: t('Расширенный поиск', 'Advanced scan') },
  ];
  const sourcePicker = <div className="source-picker"><label className="field-label" htmlFor="source-select">{t('Источник', 'Source')}</label><div className="input-action"><select id="source-select" value={sourceId} onChange={e => selectSource(e.target.value)} disabled={busy}><option value="">{t('Выберите диск или откройте образ', 'Choose a drive or open an image')}</option>{sources.map(s => <option key={s.id} value={s.id} disabled={!s.accessible}>{s.name} · {formatBytes(s.size)} · {s.path}{!s.accessible ? t(' · Нет доступа', ' · No access') : ''}</option>)}</select><button className="button secondary" disabled={busy || !native} onClick={() => void perform(pickSource)}><FolderOpen size={16} />{t('Открыть образ', 'Open image')}</button></div>{source && <p className="source-path"><HardDrive size={13} /><span>{source.path}</span><span>{formatBytes(source.size)}</span></p>}</div>;

  return <div className="app-shell">
    <aside className="sidebar">
      <a className="brand" href="#home" onClick={e => { e.preventDefault(); setSection('home'); }}><img src="/logo.svg" alt="" /><span>Memory<span className="brand-secondary">Regainer</span></span></a>
      <span className="nav-caption">{t('РАБОЧЕЕ ПРОСТРАНСТВО', 'WORKSPACE')}</span>
      <nav aria-label={t('Главная навигация', 'Main navigation')}>{nav.map(item => <button key={item.id} className={`nav-item ${section === item.id ? 'active' : ''}`} onClick={() => { setSection(item.id); if (item.id === 'advanced' && !busy) setMode('advanced'); }}><item.icon size={19} /><span>{item.title}</span>{item.id === 'recover' && !!runningKind && <span className="activity-dot" />}</button>)}</nav>
      <div className="sidebar-bottom"><div className="privacy-note"><ShieldCheck size={20} /><div><strong>{t('Ваши данные — только у вас', 'Your data stays yours')}</strong><p>{t('Без облака. Без подписки.', 'Offline. No subscription.')}</p></div></div><button className={`nav-item ${section === 'settings' ? 'active' : ''}`} onClick={() => setSection('settings')}><Settings2 size={19} />{t('Настройки', 'Settings')}</button><button className={`nav-item ${section === 'about' ? 'active' : ''}`} onClick={() => setSection('about')}><Info size={19} />{t('О приложении', 'About')}</button><div className="version">Memory Regainer <span>v0.1.2</span></div></div>
    </aside>
    <div className="workspace">
      <header className="topbar"><div className="breadcrumb">{t('Рабочее пространство', 'Workspace')}<ChevronRight size={14} /><strong>{[...nav, { id: 'settings', title: t('Настройки', 'Settings') }, { id: 'about', title: t('О приложении', 'About') }].find(n => n.id === section)?.title}</strong></div><div className="topbar-actions"><span className="offline-tag"><span />{t('Локальная обработка', 'Local processing')}</span><button className="language" aria-label={t('Switch to English', 'Переключить на русский')} onClick={() => setLang(lang === 'ru' ? 'en' : 'ru')}>{lang.toUpperCase()}</button></div></header>
      <main className={`main-content ${section === 'recover' || section === 'advanced' ? 'recovery-content' : ''}`}>
        {!native && <div className="banner warning"><Info size={18} /><span>{t('Открыта веб-версия интерфейса. Для доступа к дискам и восстановления запустите установленное приложение Memory Regainer.', 'This is the web view of the interface. Launch the installed Memory Regainer desktop app to access disks and recover files.')}</span></div>}
        {error && <div className="banner error" role="alert"><AlertTriangle size={18} /><span>{error}</span><button aria-label={t('Закрыть сообщение', 'Dismiss error')} onClick={() => setError('')}><X size={16} /></button></div>}
        {notice && <div className="banner success" role="status"><CheckCircle2 size={18} /><span>{notice}</span><button aria-label={t('Закрыть сообщение', 'Dismiss message')} onClick={() => setNotice('')}><X size={16} /></button></div>}
        {section === 'home' && <>
          <div className="hero"><div className="eyebrow"><span />{t('ПОМНИТЬ ВАЖНОЕ', 'KEEP WHAT MATTERS')}</div><h1>{t('Верните то, что важно.', 'Recover what matters.')}</h1><p>{t('Находите и восстанавливайте потерянные файлы\nс дисков, карт памяти и образов.', 'Find and restore lost files from your drives,\nmemory cards and disk images.')}</p><div className="hero-actions"><button className="button primary" onClick={() => setSection('recover')}>{t('Начать восстановление', 'Start recovery')}<ArrowRight size={17} /></button><button className="button quiet" disabled={!native || busy} onClick={() => void perform(pickSource)}><FolderOpen size={17} />{t('Открыть образ диска', 'Open disk image')}</button></div><div className="hero-mark" aria-hidden="true"><div className="orbit orbit-one" /><div className="orbit orbit-two" /><div className="hero-disk"><HardDrive size={62} strokeWidth={1.25} /><span><Check size={18} /></span></div><div className="file-float"><FileImage size={22} /></div></div></div>
          <div className="section-heading"><div><h2>{t('Доступные источники', 'Available sources')}</h2><p>{t('Выберите диск, на котором находились файлы.', 'Choose the drive where your files were stored.')}</p></div><button className="button quiet small" disabled={!native || busy} onClick={() => void perform(refreshSources)}><RefreshCw size={15} />{t('Обновить', 'Refresh')}</button></div>
          <div className="source-list">{sources.length === 0 ? <Empty icon={<HardDrive size={30} />} title={t('Нет доступных источников', 'No sources available')} description={t('Подключите накопитель или откройте локальный образ диска.', 'Connect a drive or open a local disk image.')} /> : sources.map(s => <button className="source-row" key={s.id} onClick={() => { selectSource(s.id); setSection('recover'); }} disabled={!s.accessible || busy}><span className={`source-icon ${s.kind === 'image' ? 'violet' : ''}`}>{s.kind === 'image' ? <Disc3 size={23} /> : <HardDrive size={23} />}</span><span className="source-name"><strong>{s.name}</strong><code>{s.path}</code></span><span className="source-detail">{s.detail}</span><span className="source-size">{formatBytes(s.size)}</span><span className={`source-state ${!s.accessible ? 'locked' : ''}`}>{s.accessible ? <><span />{t('Доступен', 'Available')}</> : <><LockKeyhole size={13} />{t('Нет доступа', 'No access')}</>}</span><ChevronRight size={17} /></button>)}</div>
          <div className="home-bottom"><div className="guidance"><ShieldCheck size={24} /><div><h3>{t('Сохраните шанс на восстановление', 'Protect your chance of recovery')}</h3><p>{t('Не записывайте новые данные на пострадавший диск. Сохраняйте найденные файлы на другой физический накопитель.', 'Avoid writing new data to the affected drive. Save recovered files to a different physical drive.')}</p></div></div><button className="image-entry" onClick={() => setSection('images')}><Disc3 size={23} /><span><strong>{t('Сначала создайте образ', 'Start with a disk image')}</strong><small>{t('Работайте с копией носителя', 'Work from a copy of your media')}</small></span><ArrowRight size={18} /></button></div>
        </>}
        {(section === 'recover' || section === 'advanced') && <>
          <PageTitle title={t('Восстановление файлов', 'Recover files')} text={t('Только чтение исходного носителя. Результаты основаны на найденных данных.', 'Read-only source access. Results reflect bytes actually found.')} />
          <div className="steps"><span className={source ? 'done' : 'current'}><b>{source ? <Check size={13} /> : '1'}</b>{t('Источник', 'Source')}</span><span className={scan ? 'done' : source ? 'current' : ''}><b>{scan ? <Check size={13} /> : '2'}</b>{t('Поиск', 'Scan')}</span><span className={scan?.status === 'completed' ? 'current' : ''}><b>3</b>{t('Просмотр и сохранение', 'Review & recover')}</span></div>
          <section className="scan-setup">{sourcePicker}<fieldset className="scan-modes"><legend className="field-label">{t('Режим поиска', 'Scan mode')}</legend>{[{ id: 'quick', title: t('Быстрый', 'Quick'), text: t('Удалённые записи FAT/exFAT', 'Deleted FAT/exFAT records'), icon: FileSearch }, { id: 'deep', title: t('Глубокий', 'Deep'), text: t('Поиск по сигнатурам', 'File signature carving'), icon: Search }, { id: 'advanced', title: t('Расширенный', 'Advanced'), text: t('Выбор диапазона байтов', 'Custom byte range'), icon: SlidersHorizontal }].map(m => <label key={m.id} className={`mode-option ${mode === m.id ? 'selected' : ''}`}><input type="radio" name="mode" value={m.id} checked={mode === m.id} onChange={() => setMode(m.id)} disabled={busy} /><m.icon size={19} /><span><strong>{m.title}</strong><small>{m.text}</small></span><span className="radio-indicator" /></label>)}</fieldset>
          {mode === 'advanced' && <div className="range-fields"><label>{t('Начало, байт (включительно)', 'Start byte (inclusive)')}<input inputMode="numeric" placeholder="0" value={rangeStart} onChange={e => setRangeStart(e.target.value)} disabled={busy} /></label><label>{t('Конец, байт (не включительно)', 'End byte (exclusive)')}<input inputMode="numeric" placeholder={source?.size ? String(source.size) : t('Конец источника', 'End of source')} value={rangeEnd} onChange={e => setRangeEnd(e.target.value)} disabled={busy} /></label></div>}
          {mode !== 'quick' && <div className="signature-filter"><span className="field-label">{t('Сигнатуры', 'Signatures')}</span>{['jpg', 'png', 'pdf', 'gif', 'zip', 'bmp', 'wav'].map(ext => <label key={ext}><input type="checkbox" checked={types.includes(ext)} onChange={e => setTypes(previous => e.target.checked ? [...previous, ext] : previous.filter(x => x !== ext))} disabled={busy} />{ext.toUpperCase()}</label>)}<small>{t('Не выбрано = все доступные', 'None selected = all available')}</small></div>}
          <div className="setup-footer"><p><Info size={15} />{t('Имя и целостность файла могут быть утрачены. Результат не гарантирован.', 'Names or file contents may be lost. Recovery is not guaranteed.')}</p><button className="button primary" disabled={!native || !source?.accessible || busy} onClick={() => void perform(startScan)}>{pending ? <LoaderCircle size={16} className="spin" /> : <Search size={16} />}{t('Найти файлы', 'Find files')}</button></div></section>
          {scan && <>
            <div ref={scanProgressAnchor}><JobProgress job={scan} active={scanActive} monitor={scanActive ? liveMonitor : undefined} lang={lang} onCancel={() => void requestCancellation()} /></div>
            <ScanInformation job={scan} lang={lang} />
            {scan.warnings.length > 0 && <details className="warnings"><summary><AlertTriangle size={15} />{t('Предупреждения', 'Warnings')} ({scan.warnings.length})</summary>{scan.warnings.map((w, i) => <p key={i}>{w}</p>)}</details>}
            <div className="results-heading"><div><h2>{t('Найденные файлы', 'Found files')}<span>{entries.length.toLocaleString()}</span></h2><p className="results-source">{scan.sourceName || scanSourceName}</p></div><button className="button quiet small" disabled={busy || scan.status !== 'completed'} onClick={() => void perform(exportManifest)}><ArrowDownToLine size={15} />{t('Сохранить сессию', 'Save session')}</button></div>
            <div className="results-workspace"><div className="results-main"><div className="results-toolbar"><div className="search-input"><Search size={16} /><input aria-label={t('Поиск по имени', 'Search filenames')} placeholder={t('Поиск по имени файла', 'Search by filename')} value={query} onChange={e => setQuery(e.target.value)} /></div><select aria-label={t('Тип файла', 'File type')} value={kind} onChange={e => setKind(e.target.value)}><option value="">{t('Все типы', 'All types')}</option>{kinds.map(k => <option key={k} value={k}>{k.toUpperCase()}</option>)}</select><select aria-label={t('Сортировка', 'Sort results')} value={sort} onChange={e => setSort(e.target.value)}><option value="name">{t('По имени', 'Name')}</option><option value="size">{t('По размеру', 'Size')}</option><option value="offset">{t('По смещению', 'Offset')}</option></select></div>
            <FileTable entries={filtered} selected={selected} setSelected={setSelected} focused={focused?.id} setFocused={setFocused} lang={lang} />
            <div className="results-footer"><span>{t('Выбрано', 'Selected')}: <strong>{selected.size.toLocaleString()}</strong><span className="dot-separator">·</span>{formatBytes(selectedBytes)}</span><button className="button primary" disabled={!selected.size || busy} onClick={() => setShowRecovery(true)}><ArrowDownToLine size={16} />{t('Восстановить', 'Recover')}</button></div></div>
            <aside className="preview-pane"><div className="preview-heading"><h3>{t('Просмотр файла', 'File preview')}</h3>{focused && <button aria-label={t('Закрыть просмотр', 'Close preview')} onClick={() => setFocused(null)}><X size={16} /></button>}</div>{!focused ? <Empty icon={<FileImage size={28} />} title={t('Загляните внутрь', 'Take a closer look')} description={t('Выберите имя файла для просмотра данных и оценки состояния.', 'Select a filename to inspect its contents and recovery evidence.')} /> : <><div className="preview-tabs"><button className={previewTab === 'preview' ? 'active' : ''} onClick={() => setPreviewTab('preview')}>{t('Просмотр', 'Preview')}</button><button className={previewTab === 'hex' ? 'active' : ''} onClick={() => setPreviewTab('hex')}>HEX</button></div><div className="preview-content">{previewError ? <p className="preview-error">{previewError}</p> : !preview ? <LoaderCircle className="spin" size={24} aria-label={t('Загрузка', 'Loading')} /> : previewTab === 'hex' ? <pre className="hex">{preview.hex || t('Нет данных', 'No bytes available')}</pre> : safeImageUrl(preview.dataUrl) ? <img src={safeImageUrl(preview.dataUrl)!} alt={focused.name} /> : <pre className="text-preview">{preview.text || t('Нет текстового или графического предпросмотра. Откройте HEX для просмотра байтов.', 'No text or image preview available. Open HEX to inspect the bytes.')}</pre>}</div><p className="preview-limits">{t(`Предпросмотр ограничен: изображение ≈ ${formatBytes(1_048_576)}, текст ≈ ${formatBytes(65_536)}, HEX ≈ ${formatBytes(1_024)}.`, `Preview limits: image ≈ ${formatBytes(1_048_576)}, text ≈ ${formatBytes(65_536)}, HEX ≈ ${formatBytes(1_024)}.`)}</p><div className="file-metadata"><h4>{focused.name}</h4><dl><dt>{t('Размер', 'Size')}</dt><dd>{formatBytes(focused.size)}</dd><dt>{t('Тип', 'Type')}</dt><dd>{focused.kind.toUpperCase()}</dd><dt>{t('Уверенность', 'Confidence')}</dt><dd><Confidence value={focused.confidence} lang={lang} /></dd><dt>{t('Метод', 'Method')}</dt><dd>{focused.method}</dd><dt>{t('Смещение', 'Offset')}</dt><dd>{focused.offset.toLocaleString()} B</dd><dt>{t('Фрагменты', 'Extents')}</dt><dd>{focused.extents.length}</dd></dl>{focused.notes.map((note, i) => <p className="metadata-note" key={i}>{note}</p>)}</div></>}</aside></div>
          </>}
          {!scan && <div className="scan-empty"><FileSearch size={31} /><h3>{t('Начните с поиска', 'Start with a scan')}</h3><p>{t('Здесь появятся файлы, найденные движком восстановления.', 'Files discovered by the recovery engine will appear here.')}</p></div>}
        </>}
        {section === 'images' && <>
          <PageTitle title={t('Образы дисков', 'Disk images')} text={t('Создайте побайтовую копию и ищите файлы, не обращаясь повторно к исходному носителю.', 'Create a byte-for-byte copy and scan it without repeatedly reading the original media.')} />
          <div className="two-column"><section className="panel"><div className="panel-icon"><Disc3 size={26} /></div><h2>{t('Создать RAW-образ', 'Create a RAW image')}</h2><p className="muted">{t('Копируется весь доступный диапазон источника. Потребуется место не меньше его объёма.', 'Copies the entire accessible source. The destination needs at least as much free space as the source size.')}</p>{sourcePicker}<label className="field-label">{t('Сохранить образ', 'Save image to')}</label><div className="destination-path"><code>{imageDestination || t('Путь ещё не выбран', 'No path selected')}</code><button className="button secondary small" disabled={!native || busy} onClick={() => void perform(async () => { const path = await invoke<string | null>('pick_image_destination'); if (path) setImageDestination(path); })}><FolderOpen size={16} />{t('Выбрать', 'Choose')}</button></div><button className="button primary" disabled={!native || busy || !source?.accessible || !imageDestination} onClick={() => void perform(async () => { await launchJob('image', 'image', source?.name || '', () => invoke<string>('create_image', { sourceId, destination: imageDestination })); })}><Disc3 size={16} />{t('Создать образ', 'Create image')}</button></section><section className="imaging-advice"><ShieldCheck size={30} /><h2>{t('Работайте с копией', 'Work from a copy')}</h2><p>{t('Образ можно открыть в разделе восстановления. Оригинальный источник всегда открывается только для чтения.', 'Open the resulting image in Recover files. The original source is always opened read-only.')}</p><hr /><h3>{t('Если накопитель повреждён', 'If your drive is failing')}</h3><p>{t('При повторяющихся ошибках чтения прекратите работу. Этот выпуск не заменяет специализированное оборудование или профессиональное восстановление.', 'Stop when read errors repeat. This release does not replace dedicated imaging equipment or professional data recovery.')}</p><p className="small-text">{t('Возобновление повреждённого образа и пропуск плохих секторов пока не поддерживаются.', 'Resuming incomplete images and skipping bad sectors are not yet supported.')}</p></section></div>
        </>}
        {section === 'sessions' && <>
          <PageTitle title={t('Сохранённые сессии', 'Saved sessions')} text={t('Манифесты сохраняют источник и настройки. При открытии выполняется новый поиск по исходному образу.', 'Manifests save the source and settings. Reopening runs a new scan of the original image.')} />
          <div className="section-heading"><p className="muted">{t('Здесь сохраняются сессии образов дисков. Манифесты физических носителей только экспортируются. Прерванный поиск не возобновляется.', 'Disk-image sessions appear here. Physical-device manifests are exported only. Interrupted scans are not resumed.')}</p><button className="button quiet small" disabled={!native || busy} onClick={() => void perform(refreshSessions)}><RefreshCw size={15} />{t('Обновить', 'Refresh')}</button></div>
          <div className="source-list">{!sessions.length ? <Empty icon={<History size={32} />} title={t('Сохранённых сессий пока нет', 'No saved sessions yet')} description={t('После поиска нажмите «Сохранить сессию», чтобы вернуться к результатам.', 'After a scan, choose Save session to revisit your results.')} /> : sessions.map(session => <button key={session.id} className="session-row" disabled={busy} onClick={() => void perform(async () => { setScanSourceName(session.sourceName); setSourceId(''); setSelected(new Set()); setFocused(null); setTask(null); setSection('recover'); await launchJob('scan', 'quick', session.sourceName, () => invoke<string>('load_session', { id: session.id })); await refreshSources(); })}><span className="source-icon"><History size={22} /></span><span className="source-name"><strong>{session.sourceName}</strong><small>{formatSessionDate(session.date, lang)}</small></span><span>{session.count.toLocaleString()} {t('файлов', 'files')}</span><ChevronRight size={17} /></button>)}</div>
        </>}
        {section === 'settings' && <>
          <PageTitle title={t('Настройки', 'Settings')} text={t('Удобная работа без учётной записи и подключения к сети.', 'Make yourself at home. No account or network connection required.')} />
          <section className="settings-panel"><div className="setting-row"><div><h3>{t('Язык интерфейса', 'Interface language')}</h3><p>{t('Изменение применяется сразу.', 'Changes take effect immediately.')}</p></div><select aria-label={t('Язык интерфейса', 'Interface language')} value={lang} onChange={e => setLang(e.target.value as Lang)}><option value="ru">Русский</option><option value="en">English</option></select></div><div className="setting-row"><div><h3>{t('Внешний вид', 'Appearance')}</h3><p>{t('Выберите светлую, тёмную или системную тему.', 'Use light, dark, or your system appearance.')}</p></div><div className="theme-choices">{[{ id: 'light', label: t('Светлая', 'Light'), icon: Sun }, { id: 'dark', label: t('Тёмная', 'Dark'), icon: Moon }, { id: 'system', label: t('Система', 'System'), icon: Settings2 }].map(item => <button key={item.id} aria-pressed={theme === item.id} className={theme === item.id ? 'active' : ''} onClick={() => setTheme(item.id)}><item.icon size={17} />{item.label}</button>)}</div></div><div className="setting-row"><div><h3>{t('Конфиденциальность', 'Privacy')}</h3><p>{t('Файлы не отправляются в сеть. Аналитика и реклама отсутствуют.', 'Files are never uploaded. No analytics or advertising.')}</p></div><span className="privacy-badge"><ShieldCheck size={17} />{t('Локально', 'Offline')}</span></div></section>
        </>}
        {section === 'about' && <>
          <div className="about-header"><img src="/logo.svg" alt="" /><h1>Memory Regainer</h1><p>Your files. Your memories. Recovered.</p><span className="release-badge">v0.1.2 · {t('Первый функциональный выпуск', 'Initial functional release')}</span></div><div className="about-body"><h2>{t('Бесплатное восстановление без искусственных лимитов', 'Free recovery without artificial limits')}</h2><p>{t('Независимый проект с открытым исходным кодом. Без подписки, активации, регистрации и ограничений на объём восстановленных данных.', 'An independent, open-source project. No subscription, activation, registration, or recovery-volume limits.')}</p><h3>{t('Возможности этого выпуска', 'Capabilities in this release')}</h3><p>{t('Чтение образов и доступных носителей, поиск удалённых записей FAT и exFAT, поиск файлов по реализованным сигнатурам. В exFAT поддерживаются только непрерывные, полностью записанные файлы в чистых томах exFAT 1.0 с одной FAT; свободные кластеры проверяются по bitmap. Уверенность низкая. Точные сценарии описаны в README и отчётах тестов.', 'Reads images and accessible media, scans deleted FAT and exFAT records, and carves files using implemented signatures. exFAT recovery covers only contiguous, fully initialized files on clean exFAT 1.0 volumes with one FAT; clusters are checked against the allocation bitmap. Confidence is low. Exact supported scenarios are documented in the README and test reports.')}</p><h3>{t('Технические ограничения', 'Technical limits')}</h3><p>{t('Перезаписанные, стёртые командой TRIM или недоступные зашифрованные данные не могут быть гарантированно восстановлены. Наличие сигнатуры не доказывает целостность файла. Восстановление по метаданным APFS, NTFS и ext4 не поддерживается. Фрагментированные и частично инициализированные файлы exFAT не поддерживаются.', 'Overwritten, TRIM-erased, or inaccessible encrypted data cannot be guaranteed recoverable. A signature alone does not prove file integrity. APFS, NTFS, and ext4 metadata recovery is not supported. Fragmented and partially initialized exFAT files are not supported.')}</p><div className="about-license"><span>GPL-3.0-or-later</span><span>{t('Независим от коммерческих поставщиков ПО для восстановления.', 'Not affiliated with commercial data recovery vendors.')}</span></div></div>
        </>}
        {task && (section === 'recover' || section === 'images' || section === 'advanced') && <section className="operation-section"><h2>{task.report ? t('Отчёт операции', 'Operation report') : t('Выполнение операции', 'Operation progress')}</h2><JobProgress job={task} active={taskActive} monitor={taskActive ? liveMonitor : undefined} lang={lang} onCancel={() => void requestCancellation()} />{task.report && <div className="recovery-report"><p className="verification-note">{t('Проверка SHA-256 подтверждает точность копирования доступных байтов, но не целостность исходного файла до удаления.', 'SHA-256 verification confirms the copied bytes, not the integrity of the original file before deletion.')}</p><div className="report-stats"><span><strong>{task.report.recovered}</strong>{t('Записано файлов', 'Files written')}</span><span><strong>{formatBytes(task.report.bytes)}</strong>{t('Записано данных', 'Bytes written')}</span><span className={task.report.failed ? 'failure-text' : ''}><strong>{task.report.failed}</strong>{t('Не завершено', 'Not completed')}</span></div><p><FolderOpen size={16} /><code>{task.report.destination}</code></p><details><summary>{t('Файлы и контрольные суммы SHA-256', 'Files and SHA-256 hashes')}</summary>{task.report.files.map((file, index) => <div className="report-file" key={index}><strong>{file.name}</strong><span>{file.status}</span><code>{file.sha256}</code>{file.error && <p className="failure-text">{file.error}</p>}</div>)}</details></div>}</section>}
      </main>
      <footer className="statusbar"><span><ShieldCheck size={13} />{t('Исходный носитель: только чтение', 'Source media: read-only')}</span>{runningKind && running ? <button onClick={() => setSection(runningKind === 'image' ? 'images' : 'recover')}><LoaderCircle className="spin" size={13} />{t('Операция выполняется', 'Operation in progress')}<ArrowRight size={13} /></button> : <span>{t('Без ограничений на объём восстановления', 'No recovery-volume limit')}</span>}</footer>
    </div>
    <dialog ref={recoveryDialog} className="recovery-dialog" onCancel={() => setShowRecovery(false)} aria-labelledby="recover-title"><div className="dialog-heading"><h2 id="recover-title">{t('Сохранить найденные файлы', 'Save recovered files')}</h2><button aria-label={t('Закрыть', 'Close')} onClick={() => setShowRecovery(false)}><X size={20} /></button></div>{error && <div className="banner error" role="alert"><AlertTriangle size={18} /><span>{error}</span></div>}<p>{t('Выбрано файлов', 'Selected files')}: <strong>{selected.size}</strong> · {formatBytes(selectedBytes)}</p><div className="banner warning"><ShieldCheck size={20} /><span>{t('Выберите другой физический накопитель. Запись на исходный диск может уничтожить оставшиеся данные.', 'Choose a different physical drive. Writing to the source drive could destroy remaining recoverable data.')}</span></div><label className="field-label">{t('Папка назначения', 'Destination folder')}</label><div className="destination-path"><code>{destination || t('Папка ещё не выбрана', 'No folder selected')}</code><button className="button secondary" onClick={() => void perform(async () => { const path = await invoke<string | null>('choose_destination'); if (path) setDestination(path); })}><FolderOpen size={16} />{t('Выбрать', 'Choose')}</button></div><p className="small-text">{t('Существующие файлы не перезаписываются. Контрольные суммы подтверждают точность копирования, а не целостность исходного файла.', 'Existing files are not overwritten. Checksums verify the copy, not the integrity of the original file.')}</p><div className="dialog-footer"><button className="button secondary" onClick={() => setShowRecovery(false)}>{t('Отмена', 'Cancel')}</button><button className="button primary" disabled={!destination || busy || !scan} onClick={() => void perform(async () => { if (!scan) return; setShowRecovery(false); await launchJob('recovery', 'recover', scan.sourceName || scanSourceName, () => invoke<string>('recover', { jobId: scan.id, entryIds: [...selected], destination })); })}><ArrowDownToLine size={16} />{t('Восстановить файлы', 'Recover files')}</button></div></dialog>
  </div>;
}

function ScanInformation({ job, lang }: { job: Job; lang: Lang }) {
  const t = (ru: string, en: string) => lang === 'ru' ? ru : en;
  const partitions = job.partitions ?? [];
  const methodNotes = job.methodNotes ?? [];
  if (!partitions.length && !methodNotes.length) return null;
  return <details className="scan-information">
    <summary><Info size={15} />{t('Сведения о сканировании', 'Scan information')}</summary>
    {partitions.length > 0 && <div className="partition-table"><table>
      <caption>{t('Обнаруженные разделы', 'Detected partitions')}</caption>
      <thead><tr><th>{t('Раздел', 'Partition')}</th><th>{t('Файловая система', 'Filesystem')}</th><th>{t('Размер', 'Size')}</th><th>{t('Смещение, байт', 'Offset, bytes')}</th><th>{t('Схема', 'Scheme')}</th></tr></thead>
      <tbody>{partitions.map((partition, index) => <tr key={`${partition.offset}-${index}`}><td>{partition.name || t('Без имени', 'Unnamed')}</td><td>{partition.filesystem || t('Не определена', 'Unknown')}</td><td>{formatBytes(partition.size)}</td><td>{partition.offset.toLocaleString()}</td><td>{partition.scheme || '—'}</td></tr>)}</tbody>
    </table></div>}
    {methodNotes.length > 0 && <div className="method-notes"><h3>{t('Особенности метода', 'Method details')}</h3>{methodNotes.map((note, index) => <p key={index}>{note}</p>)}</div>}
  </details>;
}

function PageTitle({ title, text }: { title: string; text: string }) { return <div className="page-title"><h1>{title}</h1><p>{text}</p></div>; }
function Empty({ icon, title, description }: { icon: React.ReactNode; title: string; description: string }) { return <div className="empty-state"><span>{icon}</span><h3>{title}</h3><p>{description}</p></div>; }
function Confidence({ value, lang }: { value: string; lang: Lang }) {
  const key = value.toLowerCase(); const ru: Record<string, string> = { high: 'Высокая', medium: 'Средняя', low: 'Низкая', unknown: 'Неизвестно' }; const en: Record<string, string> = { high: 'High', medium: 'Medium', low: 'Low', unknown: 'Unknown' };
  return <span className={`confidence ${['high', 'medium', 'low'].includes(key) ? key : 'unknown'}`}><i />{(lang === 'ru' ? ru : en)[key] || (lang === 'ru' ? 'Неизвестно' : 'Unknown')}</span>;
}

function FileTable({ entries, selected, setSelected, focused, setFocused, lang }: { entries: Entry[]; selected: Set<string>; setSelected: (ids: Set<string>) => void; focused?: string; setFocused: (entry: Entry) => void; lang: Lang }) {
  const parent = useRef<HTMLDivElement>(null);
  const virtual = useVirtualizer({ count: entries.length, getScrollElement: () => parent.current, estimateSize: () => 49, overscan: 10 });
  const t = (ru: string, en: string) => lang === 'ru' ? ru : en;
  return <div className="file-table" role="table" aria-label={t('Найденные файлы', 'Found files')} aria-rowcount={entries.length + 1}><div className="table-header table-grid" role="row"><span role="columnheader"><input type="checkbox" aria-label={t('Выбрать все видимые файлы', 'Select all visible files')} checked={selectedVisible(selected, entries)} onChange={() => setSelected(toggleVisible(selected, entries))} disabled={!entries.length} /></span><span role="columnheader">{t('Имя файла', 'Filename')}</span><span role="columnheader">{t('Размер', 'Size')}</span><span role="columnheader">{t('Уверенность', 'Confidence')}</span></div><div className="table-scroll" ref={parent}>{!entries.length ? <Empty icon={<FileSearch size={26} />} title={t('Пока нет файлов', 'No files to display')} description={t('Дождитесь поиска или измените фильтры.', 'Wait for the scan or adjust your filters.')} /> : <div style={{ height: `${virtual.getTotalSize()}px`, position: 'relative' }}>{virtual.getVirtualItems().map(item => { const entry = entries[item.index]; return <div key={entry.id} className={`file-row table-grid ${focused === entry.id ? 'focused' : ''}`} style={{ position: 'absolute', top: 0, left: 0, width: '100%', height: `${item.size}px`, transform: `translateY(${item.start}px)` }} role="row" aria-rowindex={item.index + 2}><span role="cell"><input type="checkbox" aria-label={`${t('Выбрать', 'Select')} ${entry.name}`} checked={selected.has(entry.id)} onChange={e => { const next = new Set(selected); if (e.target.checked) next.add(entry.id); else next.delete(entry.id); setSelected(next); }} /></span><span role="cell" className="filename-cell"><span className="file-type-icon">{['jpg', 'jpeg', 'png', 'gif', 'bmp', 'webp'].includes(entry.kind.toLowerCase()) ? <FileImage size={18} /> : <File size={18} />}</span><button onClick={() => setFocused(entry)} title={entry.name}>{entry.name}</button></span><span role="cell" className="file-size">{formatBytes(entry.size)}</span><span role="cell"><Confidence value={entry.confidence} lang={lang} /></span></div>; })}</div>}</div></div>;
}
