import { AlertTriangle, CheckCircle2, Clock3, LoaderCircle, Moon, RefreshCw, ShieldCheck, X } from 'lucide-react';
import type { Job, Lang } from './types';
import { formatBytes, progressPercent } from './model';
import { estimateRemainingSeconds, formatDuration, lastProgressTime } from './progress';
import type { ProgressSample } from './progress';

export interface ProgressMonitor {
  now: number;
  elapsed: number;
  lastResponseAt: number;
  pollError: string;
  progressSamples: ProgressSample[];
  cancelling: boolean;
  cancelError: string;
  launching: boolean;
  onRetry: () => void;
}

export function JobProgress({ job, active, monitor, lang, onCancel }: { job: Job; active?: boolean; monitor?: ProgressMonitor; lang: Lang; onCancel: () => void }) {
  const t = (ru: string, en: string) => lang === 'ru' ? ru : en;
  const stageLabels: Record<string, string> = {
    preparing: t('Подготовка операции', 'Preparing the operation'),
    'opening-source': t('Открываем источник только для чтения', 'Opening the source read-only'),
    partitions: t('Проверяем таблицу разделов', 'Inspecting the partition table'),
    metadata: t('Ищем удалённые записи файловой системы', 'Searching deleted filesystem records'),
    carving: t('Ищем файлы по сигнатурам', 'Searching file signatures'),
    recovering: t('Восстанавливаем выбранные файлы', 'Recovering selected files'),
    imaging: t('Создаём побайтовый образ диска', 'Creating a byte-for-byte disk image'),
    verifying: t('Проверяем данные и контрольные суммы', 'Verifying data and checksums'),
    'writing-report': t('Сохраняем отчёт операции', 'Saving the operation report'),
    cancelling: t('Останавливаем операцию', 'Stopping the operation'),
    finished: t('Все этапы операции завершены', 'All operation stages finished'),
    cancelled: t('Операция остановлена пользователем', 'Operation stopped by the user'),
    failed: t('Проверьте сообщение об ошибке', 'Review the error message'),
  };
  const metadataOnly = job.stage === 'metadata' || (job.mode === 'quick' && !job.progressDeterminate);
  const percent = job.progressDeterminate ? progressPercent(job.progress.scanned, job.progress.total) : undefined;
  const cancelling = !!monitor?.cancelling;
  const elapsed = active && monitor ? Math.max(job.elapsedSeconds || 0, monitor.elapsed) : job.elapsedSeconds || 0;
  const responseAge = monitor?.lastResponseAt ? Math.max(0, Math.floor((monitor.now - monitor.lastResponseAt) / 1000)) : undefined;
  const stale = !!active && !!monitor && !monitor.launching && (responseAge === undefined ? monitor.elapsed >= 5 : responseAge >= 5);
  const sourceBytes = job.progress.bytes_read !== undefined && Number.isFinite(job.progress.bytes_read) && job.progress.bytes_read >= 0 ? job.progress.bytes_read : undefined;
  const usesSourceActivity = !job.progressDeterminate && sourceBytes !== undefined;
  const movementAt = monitor ? lastProgressTime(monitor.progressSamples, usesSourceActivity ? 'bytesRead' : 'scanned') : undefined;
  const movementAge = movementAt !== undefined && monitor ? Math.max(0, Math.floor((monitor.now - movementAt) / 1000)) : undefined;
  const eta = active && monitor && job.progressDeterminate && !cancelling && !monitor.pollError && !stale ? estimateRemainingSeconds(monitor.progressSamples, monitor.now) : undefined;
  const label = job.status === 'running' ? cancelling ? t('Запрошена остановка', 'Stop requested') : t('Операция выполняется', 'Operation in progress') : job.status === 'completed' ? t('Обработка завершена', 'Processing finished') : job.status === 'cancelled' ? t('Остановлено — результат неполный', 'Stopped — results are incomplete') : t('Операция завершилась с ошибкой', 'Operation failed');
  const stage = job.status === 'completed' ? 'finished' : job.status === 'failed' ? 'failed' : job.status === 'cancelled' ? 'cancelled' : cancelling ? 'cancelling' : job.stage;
  const readExplanation = t('Фактически прочитанные байты, включая повторные чтения и упреждающее чтение кэша. Это не процент готовности; объём может превышать размер источника.', 'Actual bytes read, including revisits and cache read-ahead. This is not a completion percentage and may exceed the source size.');
  const sleepExplanation = t('Защита предотвращает автоматический сон при бездействии. Экран может гаснуть. Закрытие крышки, ручной сон, выключение и потеря питания не предотвращаются.', 'Prevents automatic idle sleep. The screen may dim. Closing the lid, manual sleep, shutdown, and loss of power are not prevented.');

  return <section className={`job-progress ${job.status}`} aria-label={t('Состояние операции', 'Operation status')}>
    <div className="progress-title"><span role="status" aria-live="polite">{active ? <LoaderCircle size={19} className="spin" /> : job.status === 'completed' ? <CheckCircle2 size={19} /> : <AlertTriangle size={19} />}<strong>{label}</strong></span>{percent !== undefined && <span className="progress-number">{percent.toFixed(1)}%</span>}{active && <button className="button secondary small" onClick={onCancel} disabled={cancelling}>{cancelling ? <LoaderCircle size={14} className="spin" /> : <X size={14} />}{cancelling ? t('Останавливаем…', 'Stopping…') : t('Остановить', 'Stop')}</button>}</div>
    <div className="progress-stage">{stageLabels[stage] || t('Обрабатываем данные', 'Processing data')}</div>
    {(active || percent !== undefined) && <progress max={100} value={percent} aria-label={t('Прогресс текущего этапа', 'Current stage progress')} />}
    <div className="progress-details">{!usesSourceActivity && <span>{metadataOnly ? `${t('Прочитано метаданных', 'Metadata read')}: ${formatBytes(job.progress.scanned)}` : `${t('Обработано', 'Processed')}: ${formatBytes(job.progress.scanned)}${job.progressDeterminate && job.progress.total ? ` / ${formatBytes(job.progress.total)}` : ''}`}</span>}{sourceBytes !== undefined && <span title={readExplanation}>{t('Прочитано с источника', 'Source bytes read')}: {formatBytes(sourceBytes)}</span>}{job.mode !== 'image' && <span>{job.mode === 'recover' ? t('Записано', 'Written') : t('Найдено', 'Found')}: {job.progress.found.toLocaleString()}</span>}</div>
    <div className="progress-timing"><span><Clock3 size={14} />{t('Прошло', 'Elapsed')}: <strong>{formatDuration(elapsed)}</strong></span>{active && <span>{t('Осталось на этапе', 'Stage remaining')}: <strong>{eta !== undefined ? `≈ ${formatDuration(eta)}` : job.progressDeterminate && !cancelling && !stale && !monitor?.pollError ? t('недостаточно данных для оценки', 'not enough data to estimate') : t('пока неизвестно', 'not yet known')}</strong></span>}</div>
    {active && <div className="progress-health"><span className={`engine-activity ${stale || monitor?.pollError ? 'waiting' : ''}`}><i />{monitor?.launching ? t('Запрос запуска отправлен', 'Start request sent') : monitor?.pollError ? t('Проверяем связь с движком', 'Checking engine responsiveness') : responseAge === undefined ? t('Ожидаем первый ответ движка', 'Waiting for the first engine response') : `${t('Ответ движка', 'Engine response')}: ${responseAge} ${t('с назад', 's ago')}`}</span><span className={`sleep-protection ${job.sleepInhibitionError ? 'unavailable' : ''}`} title={`${sleepExplanation}${job.sleepInhibitionError ? ` ${job.sleepInhibitionError}` : ''}`}>{job.sleepInhibited ? <ShieldCheck size={14} /> : <Moon size={14} />}{job.sleepInhibited ? t('Автосон отключён на время операции', 'Idle sleep prevented during the operation') : monitor?.launching || responseAge === undefined ? t('Статус защиты от сна уточняется', 'Checking sleep protection') : t('Защита от сна недоступна', 'Sleep protection unavailable')}</span></div>}
    {active && (monitor?.pollError || stale) && <div className="progress-issue" role="alert"><AlertTriangle size={16} /><div><strong>{t('Обновление состояния задерживается', 'Status updates are delayed')}</strong><p>{t('Операция может продолжаться. Последние полученные данные сохранены; отмена остаётся доступной.', 'The operation may still be running. The last reported data is retained, and cancellation remains available.')}</p>{monitor?.pollError && <p className="technical-detail">{monitor.pollError}</p>}</div>{monitor?.pollError && <button className="button secondary small" onClick={monitor.onRetry}><RefreshCw size={13} />{t('Повторить', 'Retry')}</button>}</div>}
    {active && monitor?.launching && monitor.elapsed >= 10 && !cancelling && <p className="progress-help">{t('Подготовка источника занимает время. Счётчик начнёт обновляться, когда движок перейдёт к чтению данных.', 'Source preparation is taking time. The counter will update when the engine starts reading data.')}</p>}
    {active && !monitor?.launching && !stale && !monitor?.pollError && !cancelling && movementAge !== undefined && movementAge >= 10 && <p className="progress-help">{usesSourceActivity ? t(`Движок отвечает. Счётчик чтения с источника не менялся как минимум ${movementAge} с; ожидается чтение или обработка данных в кэше.`, `The engine is responding. The source-read counter has not changed for at least ${movementAge} s; waiting for a read or processing cached data.`) : t(`Движок отвечает. Счётчик обработанных байтов не менялся как минимум ${movementAge} с; ожидается чтение или завершение текущего этапа.`, `The engine is responding. The processed-byte counter has not changed for at least ${movementAge} s; waiting for a read or the current stage to finish.`)}</p>}
    {active && cancelling && <p className="progress-help">{monitor?.launching ? t('Отмена поставлена в очередь и будет передана, как только движок вернёт идентификатор операции.', 'Cancellation is queued and will be sent as soon as the engine returns the operation ID.') : t('Запрос остановки отправлен. Завершение текущего системного чтения может занять время.', 'Stop has been requested. An in-flight system read may take time to return.')}</p>}
    {monitor?.cancelError && <div className="progress-issue" role="alert"><AlertTriangle size={16} /><div><strong>{t('Не удалось отправить запрос остановки', 'Could not send the stop request')}</strong><p>{monitor.cancelError}</p></div></div>}
    {active && job.sleepInhibitionError && <p className="progress-help">{t('Не дайте компьютеру перейти в сон до завершения операции.', 'Keep the computer awake until the operation finishes.')} <span className="technical-detail">{job.sleepInhibitionError}</span></p>}
    {job.error && (job.status === 'failed' || job.status === 'cancelled') && <p className={job.status === 'failed' ? 'failure-text cancel-detail' : 'cancel-detail'}>{job.error}</p>}
  </section>;
}
