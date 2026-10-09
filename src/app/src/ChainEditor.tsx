import { memo, useEffect, useRef, useState, type KeyboardEvent as ReactKeyboardEvent } from 'react';
import { DragDropProvider, DragOverlay, useDraggable, useDroppable, type DragMoveEvent, type DragOverEvent, type DragStartEvent, type DragEndEvent } from '@dnd-kit/react';
import { useSortable } from '@dnd-kit/react/sortable';
import { Accessibility, AutoScroller, PointerSensor, PointerActivationConstraints } from '@dnd-kit/dom';
import { OptimisticSortingPlugin } from '@dnd-kit/dom/sortable';
import { Plus, Power, Ellipsis, Copy, Trash2, ArrowLeft, ArrowRight, Mic, Cable } from 'lucide-react';
import { definitions, moveFilter, type FilterDefinition, type FilterInstance } from './model';
import { FilterIcon, Menu, Summary, useReducedMotion } from './ui';

export type CardAction = 'bypass' | 'duplicate' | 'delete' | 'previous' | 'next';
type Drag = { node: FilterInstance; library: boolean; index: number; over: boolean };

function LibraryItem({ definition, disabled, onAdd }: { definition: FilterDefinition; disabled: boolean; onAdd: () => void }) {
  const { ref, isDragging } = useDraggable({ id: 'library:' + definition.type, type: 'library', data: { type: definition.type, name: definition.name }, disabled });
  return <button ref={ref} className={'library-card glass' + (isDragging ? ' library-dragging' : '')}
    disabled={disabled} aria-label={'添加' + definition.name} title={definition.description} onClick={onAdd}>
    <FilterIcon type={definition.type} /><span>{definition.name}</span><Plus size={17} aria-hidden="true" />
  </button>;
}

function EffectCard({ node, definition, index, selected, dragging, sourceId, reduced, onSelect, onAction, last, canDuplicate }: {
  node: FilterInstance; definition: FilterDefinition; index: number; selected: boolean; dragging: boolean; reduced: boolean;
  onSelect: (id: string) => void; onAction: (id: string, action: CardAction) => void; last: boolean; sourceId: string | null; canDuplicate: boolean;
}) {
  const { ref, handleRef, isDragSource } = useSortable({
    id: node.id, index, group: 'chain', type: 'chain-card', accept: ['chain-card', 'library'],
    data: { name: definition.name },
    plugins: defaults => defaults.filter(plugin => plugin !== OptimisticSortingPlugin),
    transition: reduced ? null : { duration: 180, easing: 'cubic-bezier(.2,.8,.2,1)', idle: true },
    disabled: { droppable: sourceId === node.id },
  });
  const keys = (event: ReactKeyboardEvent) => {
    if (dragging || !event.altKey) return;
    if (['ArrowLeft', 'ArrowUp', 'ArrowRight', 'ArrowDown'].includes(event.key)) {
      event.preventDefault();
      event.stopPropagation();
      onAction(node.id, ['ArrowLeft', 'ArrowUp'].includes(event.key) ? 'previous' : 'next');
    }
  };
  return <article ref={ref} data-node-id={node.id} data-drag-source={isDragSource ? node.id : undefined}
    className={'effect-card glass' + (selected ? ' selected' : '') + (!node.enabled ? ' bypassed' : '') + (isDragSource ? ' drag-source' : '')}
    aria-label={definition.name + '滤镜'}>
    <button id={'select-' + node.id} ref={handleRef} className="card-select" aria-label={'选择' + definition.name + '滤镜'} aria-pressed={selected}
      onClick={() => { if (!dragging) onSelect(node.id); }} onKeyDown={keys}>
      <FilterIcon type={node.type} /><span className="card-name">{definition.name}</span>
      <Summary node={node} definition={definition} />
      {!node.enabled && <span className="bypass-label">已旁路</span>}
    </button>
    <div className="card-actions">
      <button className={'icon-button power-button' + (!node.enabled ? ' is-bypassed' : '')} disabled={dragging}
        aria-label={(node.enabled ? '旁路' : '启用') + definition.name} aria-pressed={node.enabled}
        title={node.enabled ? '旁路此滤镜' : '启用此滤镜'} onClick={() => onAction(node.id, 'bypass')}><Power size={17} aria-hidden="true" /></button>
      <Menu label={definition.name + '更多操作'} content={<Ellipsis size={19} aria-hidden="true" />} className="icon-button" width={188} disabled={dragging}>
        <button role="menuitem" disabled={dragging || index === 0} onClick={() => onAction(node.id, 'previous')}><ArrowLeft size={16} aria-hidden="true" />向前移动</button>
        <button role="menuitem" disabled={dragging || last} onClick={() => onAction(node.id, 'next')}><ArrowRight size={16} aria-hidden="true" />向后移动</button>
        <button role="menuitem" disabled={dragging || !canDuplicate} onClick={() => onAction(node.id, 'duplicate')}><Copy size={16} aria-hidden="true" />复制滤镜</button>
        <button role="menuitem" disabled={dragging} onClick={() => onAction(node.id, 'delete')}><Trash2 size={16} aria-hidden="true" />删除滤镜</button>
      </Menu>
    </div>
  </article>;
}

function InsertionSlot({ index, active, onPick }: { index: number; active: boolean; onPick: () => void }) {
  return <div className={'insert-slot' + (active ? ' insertion-selected' : '')}><span />
    <button aria-label={'在位置 ' + (index + 1) + ' 插入滤镜'} title="选择此位置，再点击上方滤镜" onClick={onPick}><Plus size={12} aria-hidden="true" /></button><span /></div>;
}

export const ChainEditor = memo(function ChainEditor({ chain, selectedId, onSelect, onChange, onAdd, onAction, onDraggingChange }: {
  chain: FilterInstance[]; selectedId: string | null; onSelect: (id: string) => void; onChange: (chain: FilterInstance[]) => void;
  onAdd: (definition: FilterDefinition, index: number) => void; onAction: (id: string, action: CardAction) => void; onDraggingChange: (dragging: boolean) => void;
}) {
  const reduced = useReducedMotion();
  const [drag, setDrag] = useState<Drag | null>(null);
  const dragRef = useRef<Drag | null>(null);
  const [insertAt, setInsertAt] = useState<number | null>(null);
  const viewport = useRef<HTMLDivElement>(null);
  const track = useRef<HTMLDivElement>(null);
  const frame = useRef<number | null>(null);
  const pending = useRef<DragMoveEvent['operation'] | null>(null);
  const suppressClick = useRef(false);
  const { ref: dropRef, isDropTarget } = useDroppable({ id: 'chain-surface', accept: ['library', 'chain-card'] });
  useEffect(() => {
    const element = viewport.current;
    if (!element) return;
    const wheel = (event: WheelEvent) => {
      if (event.ctrlKey || element.scrollWidth <= element.clientWidth) return;
      const delta = (event.deltaX || event.deltaY) * (event.deltaMode === 1 ? 16 : 1);
      const before = element.scrollLeft;
      element.scrollLeft += delta;
      if (before !== element.scrollLeft) event.preventDefault();
    };
    element.addEventListener('wheel', wheel, { passive: false });
    return () => element.removeEventListener('wheel', wheel);
  }, []);
  useEffect(() => {
    if (selectedId && !dragRef.current) document.getElementById('select-' + selectedId)?.scrollIntoView({ block: 'nearest', inline: 'nearest', behavior: reduced ? 'instant' : 'smooth' });
  }, [selectedId, reduced]);
  useEffect(() => () => { if (frame.current !== null) cancelAnimationFrame(frame.current); }, []);
  const inside = (operation: DragMoveEvent['operation']) => {
    const rect = viewport.current?.getBoundingClientRect();
    const point = operation.position.current;
    return Boolean(rect && point.x >= rect.left && point.x <= rect.right && point.y >= rect.top && point.y <= rect.bottom);
  };
  const updatePreview = (operation: DragMoveEvent['operation']) => {
    const current = dragRef.current;
    if (!current) return;
    const over = inside(operation);
    let index = current.index;
    if (over) {
      const point = operation.position.current;
      const cards = Array.from(track.current?.querySelectorAll<HTMLElement>('.effect-card[data-node-id]') ?? [])
        .filter(element => current.library || element.dataset.nodeId !== current.node.id);
      // Batch geometry reads, then update only when the insertion position changes.
      index = cards.map(element => element.getBoundingClientRect()).filter(rect => point.x > (rect.left + rect.right) / 2).length;
    }
    if (over !== current.over || index !== current.index) {
      dragRef.current = { ...current, index, over };
      setDrag(dragRef.current);
    }
  };
  const schedule = (event: DragMoveEvent | DragOverEvent) => {
    pending.current = event.operation;
    if (frame.current !== null) return;
    frame.current = requestAnimationFrame(() => {
      frame.current = null;
      if (pending.current) updatePreview(pending.current);
    });
  };
  const preview = drag && !drag.library ? moveFilter(chain, drag.node.id, drag.index) : chain;
  return <DragDropProvider
    sensors={[PointerSensor.configure({
      activationConstraints: [new PointerActivationConstraints.Distance({ value: 6 })],
      preventActivation: (event, source) => {
        const control = (event.target as Element).closest('button,input,select,textarea,[role="menu"]');
        return Boolean(control && control !== source.handle && control !== source.element);
      },
    })]}
    plugins={defaults => [...defaults,
      AutoScroller.configure({ acceleration: 12, threshold: { x: .12, y: 0 } }),
      Accessibility.configure({
        screenReaderInstructions: { draggable: '点击选择卡片，按 Alt 加左右方向键移动；也可使用更多菜单移动。' },
        announcements: {
          dragstart: (event: DragStartEvent) => '已拿起' + event.operation.source?.data.name + '。',
          dragend: (event: DragEndEvent) => event.canceled ? '已取消拖动，声音管线未改变。' : '拖动结束。',
        },
      }),
    ]}
    onDragStart={event => {
      const source = event.operation.source;
      if (!source) return;
      const library = source.type === 'library';
      const definition = definitions.find(d => d.type === source.data.type);
      const node = library && definition ? { id: 'preview-filter', type: definition.type, enabled: true, params: Object.fromEntries(definition.parameters.map(p => [p.key, p.default])) }
        : chain.find(item => item.id === source.id);
      if (!node) return;
      dragRef.current = { node, library, index: library ? chain.length : chain.findIndex(item => item.id === node.id), over: !library };
      setInsertAt(null);
      onDraggingChange(true);
      setDrag(dragRef.current);
    }}
    onDragMove={schedule}
    onDragOver={schedule}
    onDragEnd={event => {
      if (frame.current !== null) { cancelAnimationFrame(frame.current); frame.current = null; }
      updatePreview(event.operation);
      const current = dragRef.current;
      if (!event.canceled && current && inside(event.operation)) {
        if (current.library) {
          const definition = definitions.find(d => d.type === current.node.type);
          if (definition) onAdd(definition, current.index);
        } else if (current.index !== chain.findIndex(node => node.id === current.node.id)) onChange(moveFilter(chain, current.node.id, current.index));
      }
      dragRef.current = null;
      onDraggingChange(false);
      setDrag(null);
      suppressClick.current = true;
      setTimeout(() => { suppressClick.current = false; }, 0);
    }}>
    <section className="library-panel" aria-label="滤镜库">
      <div className="section-heading"><h2>滤镜库</h2><span>{insertAt === null ? '' : '加入位置 ' + (insertAt + 1)}</span></div>
      <div className="library-list">{definitions.map(definition => <LibraryItem key={definition.type} definition={definition} disabled={chain.length >= 64}
        onAdd={() => { if (suppressClick.current || dragRef.current) return; onAdd(definition, insertAt ?? chain.length); setInsertAt(null); }} />)}</div>
    </section>
    <section className="chain-panel" aria-label="滤镜管线">
      <div className="section-heading"><h2>声音管线</h2><span>{chain.length} 个滤镜</span><span className="chain-hint">从左到右处理</span></div>
      <div ref={element => { viewport.current = element; dropRef(element); }} className={'chain-viewport' + (isDropTarget ? ' drop-active' : '')} tabIndex={0} aria-label="横向声音管线">
        <div ref={track} className="chain-track">
          <div className="endpoint"><span className="glass"><Mic size={23} aria-hidden="true" /></span><small>输入</small></div>
          <InsertionSlot index={0} active={insertAt === 0} onPick={() => setInsertAt(0)} />
          {chain.length === 0 && !(drag?.library && drag.over) ? <button className="empty-chain" onClick={() => document.querySelector<HTMLButtonElement>('.library-card')?.focus()}>
            <Plus size={26} aria-hidden="true" /><strong>加入你的第一个滤镜</strong><span>拖入上方滤镜，或点击添加</span><small>空管线直接输出原音</small>
          </button> : null}
          {Array.from({ length: preview.length + 1 }, (_, index) => <div className="chain-fragment" key={index === preview.length ? 'tail' : preview[index].id}>
            {drag?.library && drag.over && drag.index === index ? <div className="library-placeholder" aria-hidden="true"><FilterIcon type={drag.node.type} /></div> : null}
            {index < preview.length ? <>
              <EffectCard node={preview[index]} definition={definitions.find(d => d.type === preview[index].type)!} index={index + (drag?.library && drag.over && drag.index <= index ? 1 : 0)}
                selected={selectedId === preview[index].id} dragging={Boolean(drag)} reduced={reduced} last={index === preview.length - 1}
                sourceId={drag && !drag.library ? drag.node.id : null} canDuplicate={chain.length < 64} onSelect={onSelect} onAction={onAction} />
              <InsertionSlot index={index + 1} active={insertAt === index + 1} onPick={() => setInsertAt(index + 1)} />
            </> : null}
          </div>)}
          <div className="endpoint"><span className="glass"><Cable size={23} aria-hidden="true" /></span><small>输出</small></div>
        </div>
      </div>
      <p className="sr-only">可以使用滚轮横向浏览；选择卡片后按 Alt 加左右方向键移动，或使用更多操作菜单。</p>
    </section>
    <DragOverlay className="drag-overlay" dropAnimation={reduced ? null : { duration: 180, easing: 'cubic-bezier(.2,.8,.2,1)' }}>
      {source => {
        const definition = definitions.find(d => d.type === (source.type === 'library' ? source.data.type : chain.find(item => item.id === source.id)?.type));
        const node = source.type === 'library' ? drag?.node : chain.find(item => item.id === source.id);
        return definition && node ? <div className="overlay-card"><FilterIcon type={definition.type} /><strong>{definition.name}</strong><Summary node={node} definition={definition} /></div> : null;
      }}
    </DragOverlay>
  </DragDropProvider>;
});
