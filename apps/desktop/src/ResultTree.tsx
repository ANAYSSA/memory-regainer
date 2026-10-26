import { useState } from 'react';
import { ChevronDown, ChevronRight, Folder, ListFilter } from 'lucide-react';
import type { Lang } from './types';
import type { ResultNode, TreeView } from './results';
import { toggleIds } from './results';

export function ResultTree({ nodes, view, setView, current, setCurrent, selected, setSelected, allIds, lang }: { nodes: ResultNode[]; view: TreeView; setView: (view: TreeView) => void; current: string; setCurrent: (id: string) => void; selected: Set<string>; setSelected: (ids: Set<string>) => void; allIds: string[]; lang: Lang }) {
  const [expanded, setExpanded] = useState<Set<string>>(new Set());
  const t = (ru: string, en: string) => lang === 'ru' ? ru : en;
  const render = (node: ResultNode, depth: number) => {
    const open = expanded.has(node.id);
    const checked = node.entryIds.length > 0 && node.entryIds.every(id => selected.has(id));
    const partial = !checked && node.entryIds.some(id => selected.has(id));
    return <li key={node.id}>
      <div className={`result-tree-row ${current === node.id ? 'active' : ''}`} style={{ paddingLeft: 8 + depth * 12 }}>
        {node.children.length ? <button className="tree-expand" aria-label={`${open ? t('Свернуть', 'Collapse') : t('Развернуть', 'Expand')} ${node.label}`} aria-expanded={open} onClick={() => setExpanded(previous => { const next = new Set(previous); if (open) next.delete(node.id); else next.add(node.id); return next; })}>{open ? <ChevronDown size={13} /> : <ChevronRight size={13} />}</button> : <span className="tree-spacer" />}
        <input type="checkbox" aria-label={`${t('Выбрать файлы в', 'Select files in')} ${node.label}`} checked={checked} ref={input => { if (input) input.indeterminate = partial; }} onChange={() => setSelected(toggleIds(selected, node.entryIds))} />
        <button className="tree-label" onClick={() => setCurrent(node.id)} title={node.label}><Folder size={13} /><span>{node.label}</span><small>{node.entryIds.length.toLocaleString()}</small></button>
      </div>
      {open && node.children.length > 0 && <ul>{node.children.map(child => render(child, depth + 1))}</ul>}
    </li>;
  };
  return <aside className="result-tree" aria-label={t('Группы найденных файлов', 'Found file groups')}>
    <div className="tree-tabs"><button className={view === 'location' ? 'active' : ''} onClick={() => { setView('location'); setCurrent(''); }}>{t('Расположение', 'Location')}</button><button className={view === 'type' ? 'active' : ''} onClick={() => { setView('type'); setCurrent(''); }}>{t('Тип файла', 'File type')}</button></div>
    <div className={`result-tree-row all-results ${!current ? 'active' : ''}`}><input type="checkbox" aria-label={t('Выбрать все загруженные результаты', 'Select all loaded results')} checked={allIds.length > 0 && allIds.every(id => selected.has(id))} onChange={() => setSelected(toggleIds(selected, allIds))} disabled={!allIds.length} /><button className="tree-label" onClick={() => setCurrent('')}><ListFilter size={13} /><span>{t('Все файлы', 'All files')}</span><small>{allIds.length.toLocaleString()}</small></button></div>
    <ul className="tree-roots">{nodes.map(node => render(node, 0))}</ul>
    {!nodes.length && <p className="tree-empty">{t('Группы появятся по мере обнаружения файлов.', 'Groups appear as files are found.')}</p>}
  </aside>;
}
