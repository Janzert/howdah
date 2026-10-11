<script lang="ts">
  // The position editor (docs/UI-SURVEY.md, section 6): pieces from a
  // palette per side onto the board, off it by dragging them away or with
  // a right click; the side to move and move number; the starting setup,
  // clearing and symmetry tools; typed placements as in 4steps; and the
  // short format kept in step with the board. The backend checks every
  // change (`check_position`); the frontend only lays pieces out.
  import { onMount, untrack } from 'svelte';
  import { api, errorMessage } from '../api';
  import type { Color } from '../bindings/Color';
  import type { Piece } from '../bindings/Piece';
  import type { PositionCheck } from '../bindings/PositionCheck';
  import type { PositionSpec } from '../bindings/PositionSpec';
  import type { Square } from '../bindings/Square';
  import Board from '../board/Board.svelte';
  import { BoardModel } from '../board/boardModel.svelte';
  import PieceGlyph from '../board/PieceGlyph.svelte';
  import { registerEditor } from '../devHooks';
  import { settings } from '../settings.svelte';
  import type { Theme } from '../theme';
  import { EditorBoard, letterOf, palettePieces } from './editorBoard';

  interface Props {
    initial: PositionSpec;
    theme: Theme;
    flipped: boolean;
    /** Starts free play (analysis) from the position. */
    onAnalyse: (position: PositionSpec) => Promise<string | null>;
    /** Opens the new-game dialog for the position. */
    onPlay: (position: PositionSpec) => void;
    onClose: () => void;
  }
  let { initial, theme, flipped = $bindable(), onAnalyse, onPlay, onClose }: Props = $props();

  /** The editor's own board model: the window's game keeps updating its own. */
  const model = new BoardModel();

  type Tool = { kind: 'pointer' } | { kind: 'delete' } | { kind: 'piece'; piece: Piece };

  let board = new EditorBoard();
  let side = $state<Color>('gold');
  let moveNumber = $state(2);
  let tool = $state<Tool>({ kind: 'pointer' });
  let check = $state<PositionCheck | null>(null);
  let error = $state<string | null>(null);
  /** The short-format box, while the user edits it. */
  let positionText = $state('');
  let editingText = $state(false);
  let typed = $state('');
  let busy = $state(false);
  let boardRef = $state<ReturnType<typeof Board>>();
  let checkSeq = 0;

  const spec = (): PositionSpec => board.spec(side, moveNumber);
  const marked = $derived([...new Set(check?.problems.flatMap((p) => p.squares) ?? [])]);
  const playable = $derived(check != null && check.problems.length === 0 && !busy);
  const left = $derived(new Map(check?.left ?? []));

  function load(p: PositionSpec) {
    board = EditorBoard.fromShort(p.short);
    side = p.sideToMove;
    moveNumber = p.moveNumber;
    changed();
  }

  /** Shows the board as it now stands and has the backend check it. */
  function changed() {
    model.snap(board.pieceViews());
    const seq = ++checkSeq;
    api
      .checkPosition(spec())
      .then((c) => {
        if (seq !== checkSeq) return;
        check = c;
        if (!editingText) positionText = c.short;
      })
      .catch((e) => (error = errorMessage(e)));
  }

  // The side to move and move number count as changes too.
  $effect(() => {
    void side;
    void moveNumber;
    untrack(changed);
  });

  onMount(() => {
    load(initial);
    if (import.meta.env.DEV) {
      registerEditor(() => ({
        short: check?.short ?? '',
        label: check?.label ?? '',
        problems: check?.problems.map((p) => p.message) ?? [],
        tool: tool.kind === 'piece' ? letterOf(tool.piece) : tool.kind,
      }));
      return () => registerEditor(null);
    }
  });

  function onSquareDown(sq: Square, button: number): boolean {
    if (button === 2) {
      if (board.squares[sq]) {
        board.remove(sq);
        changed();
      }
      return true;
    }
    if (button !== 0 || tool.kind === 'pointer') return false;
    const here = board.squares[sq];
    if (tool.kind === 'delete' || (here && here.kind === tool.piece.kind && here.color === tool.piece.color)) {
      board.remove(sq);
    } else {
      board.put(sq, tool.piece);
    }
    changed();
    return true;
  }

  function onDrop(from: Square, to: Square): Promise<boolean> {
    board.move(from, to);
    changed();
    return Promise.resolve(true);
  }

  function onDropOff(from: Square) {
    board.remove(from);
    changed();
  }

  // Dragging from the palette: a ghost follows the pointer, and letting go
  // over the board places the piece. A press without moving picks the tool.
  let paletteDrag = $state<{ piece: Piece; x: number; y: number; moved: boolean } | null>(null);
  let pressAt = { x: 0, y: 0 };

  function paletteDown(e: PointerEvent, piece: Piece) {
    if (e.button !== 0) return;
    e.preventDefault();
    pressAt = { x: e.clientX, y: e.clientY };
    paletteDrag = { piece, x: e.clientX, y: e.clientY, moved: false };
  }

  function windowMove(e: PointerEvent) {
    if (!paletteDrag) return;
    paletteDrag.x = e.clientX;
    paletteDrag.y = e.clientY;
    if (Math.hypot(e.clientX - pressAt.x, e.clientY - pressAt.y) > 5) paletteDrag.moved = true;
  }

  function windowUp(e: PointerEvent) {
    const d = paletteDrag;
    if (!d) return;
    paletteDrag = null;
    if (!d.moved) {
      const same = tool.kind === 'piece' && tool.piece.kind === d.piece.kind && tool.piece.color === d.piece.color;
      tool = same ? { kind: 'pointer' } : { kind: 'piece', piece: d.piece };
      return;
    }
    const sq = boardRef?.squareAtClient(e.clientX, e.clientY);
    if (sq != null) {
      board.put(sq, d.piece);
      changed();
    }
  }

  async function startingSetup() {
    load({ ...(await api.defaultPosition()), moveNumber });
  }

  function clear() {
    board.clear();
    changed();
  }

  function mirror() {
    board.mirror();
    changed();
  }

  function swapColors() {
    board.swapColors();
    side = side === 'gold' ? 'silver' : 'gold';
    changed();
  }

  /** Reads the position box (Enter, or leaving it). */
  async function readText() {
    editingText = false;
    if (check && positionText.trim() === check.short) return;
    try {
      load(await api.parsePosition(positionText));
      error = null;
    } catch (e) {
      error = errorMessage(e);
    }
  }

  async function applyTyped() {
    if (!typed.trim()) return;
    try {
      load(await api.editPosition(spec(), typed));
      typed = '';
      error = null;
    } catch (e) {
      error = errorMessage(e);
    }
  }

  async function copy(text: string | undefined) {
    if (!text) return;
    try {
      await navigator.clipboard.writeText(text);
    } catch (e) {
      error = errorMessage(e);
    }
  }

  async function analyse() {
    busy = true;
    error = await onAnalyse(spec());
    busy = false;
  }

  function isTool(t: Tool, piece?: Piece): boolean {
    if (piece) return tool.kind === 'piece' && tool.piece.kind === piece.kind && tool.piece.color === piece.color;
    return tool.kind === t.kind;
  }
</script>

<svelte:window onpointermove={windowMove} onpointerup={windowUp} />

{#snippet palette(color: Color)}
  <div class="palette" role="toolbar" aria-label="{color} pieces">
    {#each palettePieces(color) as piece (piece.kind)}
      {@const n = left.get(letterOf(piece)) ?? 0}
      <button
        class="spare"
        class:selected={isTool(tool, piece)}
        aria-pressed={isTool(tool, piece)}
        aria-label="{color} {piece.kind}, {n} left"
        title="{color} {piece.kind}: drag onto the board, or click to place with clicks"
        onpointerdown={(e) => paletteDown(e, piece)}
        onkeydown={(e) => {
          if (e.key === 'Enter' || e.key === ' ') {
            e.preventDefault();
            tool = isTool(tool, piece) ? { kind: 'pointer' } : { kind: 'piece', piece };
          }
        }}
      >
        <svg viewBox="0 0 100 100" aria-hidden="true"><PieceGlyph {piece} {theme} /></svg>
        <span class="count" class:over={n < 0} class:none={n === 0}>{n}</span>
      </button>
    {/each}
  </div>
{/snippet}

<main class="editor" aria-label="Position editor">
  <section class="board-area">
    {@render palette(flipped ? 'gold' : 'silver')}
    <div class="board-wrap">
      <div
        class="board-box"
        class:placing={tool.kind === 'piece'}
        class:deleting={tool.kind === 'delete'}
      >
        <Board
          bind:this={boardRef}
          {model}
          {theme}
          {flipped}
          interactive={true}
          pushPending={null}
          lastMove={null}
          coordinates={settings.coordinates === 'none' ? 'traps' : settings.coordinates}
          hoverInput="off"
          positionKey=""
          {marked}
          onStep={() => Promise.resolve(false)}
          {onDrop}
          {onDropOff}
          {onSquareDown}
          legalTargets={() => Promise.resolve([])}
          planRoute={() => Promise.resolve(null)}
        />
      </div>
    </div>
    {@render palette(flipped ? 'silver' : 'gold')}
  </section>

  <aside class="panel">
    <h2>Position editor</h2>
    <div class="tools-row" role="toolbar" aria-label="Tools">
      <button aria-pressed={isTool({ kind: 'pointer' })} onclick={() => (tool = { kind: 'pointer' })} title="Drag pieces to move them, or off the board to remove them">
        Move
      </button>
      <button aria-pressed={isTool({ kind: 'delete' })} onclick={() => (tool = { kind: 'delete' })} title="Click pieces to remove them (a right click always does)">
        Remove
      </button>
    </div>

    <fieldset class="side">
      <legend>To move</legend>
      <label><input type="radio" bind:group={side} value="gold" /> <span class="dot gold"></span> Gold</label>
      <label><input type="radio" bind:group={side} value="silver" /> <span class="dot silver"></span> Silver</label>
      <label class="number" title="The move number of the first turn (2 is the first turn after the setups)">
        Move <input type="number" min="2" bind:value={moveNumber} onchange={() => (moveNumber = Math.max(2, Math.floor(moveNumber || 2)))} />
      </label>
    </fieldset>

    <div class="tools-row">
      <button onclick={startingSetup} title="Both sides' default setups">Starting setup</button>
      <button onclick={clear}>Clear board</button>
      <button onclick={() => (flipped = !flipped)} title="Turn the board around">Flip</button>
      <button onclick={mirror} title="Swap the a and h files (the same position, mirrored)">Mirror</button>
      <button onclick={swapColors} title="Swap gold and silver and turn the board over, with the other side to move: the same position for the other color">
        Swap colors
      </button>
    </div>

    <div class="status" role="status" aria-live="polite">
      {#if check == null}
        Checking…
      {:else if check.problems.length === 0}
        <span class="ok">Ready: {check.label}, {side} to move</span>
      {:else}
        <ul class="problems" aria-label="Problems">
          {#each check.problems as p (p.message)}<li>{p.message}</li>{/each}
        </ul>
      {/if}
    </div>

    <label class="field">
      Position (short format)
      <textarea
        class="mono"
        rows="3"
        bind:value={positionText}
        spellcheck="false"
        onfocus={() => (editingText = true)}
        onblur={readText}
        onkeydown={(e) => {
          if (e.key === 'Enter' && !e.shiftKey) {
            e.preventDefault();
            void readText();
          }
          if (e.key === 'Escape') {
            editingText = false;
            positionText = check?.short ?? '';
          }
        }}
      ></textarea>
    </label>
    <div class="tools-row">
      <button onclick={() => copy(check?.short)} title="Copy the short format, as AEI's setposition and the record's Position tag take it">Copy</button>
      <button onclick={() => copy(check?.long)} title="Copy the board diagram (the long format)">Copy diagram</button>
      <button
        onclick={() => copy(check?.setup)}
        title="Copy as two setup moves placing every piece (and 2g pass when silver moves first), as arimaa.com's puzzle pages write positions. The move number isn't kept."
      >
        Copy as setup moves
      </button>
    </div>
    <p class="hint">
      Paste a short position (<code>g [rrrrrrrr…]</code>), a board diagram or setup moves (<code>1g …</code>, <code>1s …</code>),
      then press Enter.
    </p>

    <label class="field">
      Type pieces
      <input
        class="mono"
        bind:value={typed}
        placeholder="Ed4 rc6 Ra2x Hb2n s"
        spellcheck="false"
        onkeydown={(e) => {
          if (e.key === 'Enter') void applyTyped();
        }}
      />
    </label>
    <p class="hint"><code>Ed4</code> places, <code>Hb2n</code> moves, <code>Ra2x</code> removes, <code>g</code>/<code>s</code> sets the side to move.</p>

    {#if error}<p class="error" role="alert">{error}</p>{/if}

    <div class="spacer"></div>
    <div class="actions">
      <button class="primary" disabled={!playable} onclick={analyse} title="Free play from this position, to analyse it">
        Analyse
      </button>
      <button disabled={!playable} onclick={() => onPlay(spec())} title="A new game from this position: against an engine, or with a clock">
        Play…
      </button>
      <button onclick={onClose} title="Leave the editor without starting from this position">Cancel</button>
    </div>
  </aside>

  {#if paletteDrag?.moved}
    <svg class="ghost" viewBox="0 0 100 100" style:left="{paletteDrag.x}px" style:top="{paletteDrag.y}px" aria-hidden="true">
      <PieceGlyph piece={paletteDrag.piece} {theme} />
    </svg>
  {/if}
</main>

<style>
  .editor {
    display: grid;
    grid-template-columns: 1fr minmax(260px, 320px);
    height: 100vh;
    overflow: hidden;
  }
  .board-area {
    display: flex;
    flex-direction: column;
    align-items: center;
    gap: 6px;
    padding: 12px 16px;
    min-width: 0;
    min-height: 0;
  }
  .board-wrap {
    flex: 1;
    width: 100%;
    container-type: size;
    display: flex;
    justify-content: center;
    align-items: center;
    min-height: 0;
  }
  .board-box {
    width: min(100cqw, 100cqh);
    height: min(100cqw, 100cqh);
  }
  .board-box.placing :global(svg) {
    cursor: copy;
  }
  .board-box.deleting :global(svg) {
    cursor: not-allowed;
  }
  .palette {
    display: flex;
    gap: 4px;
  }
  .spare {
    position: relative;
    width: 52px;
    height: 52px;
    padding: 2px;
    touch-action: none;
    cursor: grab;
  }
  .spare svg {
    width: 100%;
    height: 100%;
    pointer-events: none;
  }
  .spare.selected {
    background: var(--accent);
    border-color: var(--accent);
  }
  .count {
    position: absolute;
    right: 2px;
    bottom: 1px;
    font-size: 11px;
    font-weight: 600;
    color: var(--muted);
  }
  .count.none {
    opacity: 0.6;
  }
  .count.over {
    color: var(--warn);
  }
  .panel {
    display: flex;
    flex-direction: column;
    gap: 10px;
    padding: 12px;
    background: var(--panel);
    border-left: 1px solid var(--border);
    min-height: 0;
    overflow-y: auto;
  }
  h2 {
    margin: 0;
    font-size: 16px;
  }
  .tools-row {
    display: flex;
    flex-wrap: wrap;
    gap: 6px;
  }
  [aria-pressed='true'] {
    background: var(--accent);
    color: var(--accent-text);
  }
  .side {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    gap: 6px 12px;
    margin: 0;
    padding: 6px 8px;
    border: 1px solid var(--border);
    border-radius: 4px;
  }
  .side label {
    display: flex;
    align-items: center;
    gap: 4px;
  }
  .number input {
    width: 4.5em;
  }
  .dot {
    display: inline-block;
    width: 10px;
    height: 10px;
    border-radius: 50%;
    border: 1px solid rgba(0, 0, 0, 0.4);
  }
  .dot.gold {
    background: #e3b23c;
  }
  .dot.silver {
    background: #c9ced6;
  }
  .status {
    font-size: 13px;
    min-height: 1.5em;
  }
  .ok {
    color: var(--clock-ok);
  }
  .problems {
    margin: 0;
    padding-left: 18px;
    color: var(--warn);
  }
  .field {
    display: flex;
    flex-direction: column;
    gap: 3px;
    font-size: 13px;
  }
  .mono {
    font-family: ui-monospace, monospace;
    font-size: 12px;
  }
  textarea.mono {
    resize: vertical;
    white-space: pre-wrap;
    word-break: break-all;
  }
  .hint {
    margin: -4px 0 0;
    font-size: 12px;
    color: var(--muted);
  }
  .error {
    margin: 0;
    color: var(--warn);
    font-size: 13px;
  }
  .spacer {
    flex: 1;
  }
  .actions {
    display: flex;
    gap: 6px;
  }
  .ghost {
    position: fixed;
    width: 64px;
    height: 64px;
    transform: translate(-50%, -50%);
    pointer-events: none;
    filter: drop-shadow(0 6px 6px rgba(0, 0, 0, 0.35));
    z-index: 10;
  }
</style>
