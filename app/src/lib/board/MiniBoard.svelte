<script lang="ts">
  // A small read-only board for previews: the board and pieces of a
  // position, no input or animation.
  import type { PositionView } from '../bindings/PositionView';
  import { squareXY } from '../geometry';
  import { viewBox, type Theme } from '../theme';
  import BoardSurface from './BoardSurface.svelte';
  import PieceGlyph from './PieceGlyph.svelte';

  let { position, theme, flipped }: { position: PositionView; theme: Theme; flipped: boolean } = $props();
</script>

<svg viewBox={viewBox(theme)} preserveAspectRatio="xMidYMid meet" aria-hidden="true">
  <BoardSurface {theme} {flipped} coordinates="none" />
  {#each position.pieces as p (p.id)}
    {@const at = squareXY(p.square, flipped)}
    <g transform="translate({at.x}, {at.y})">
      <PieceGlyph piece={p.piece} {theme} />
    </g>
  {/each}
</svg>

<style>
  svg {
    display: block;
    width: 100%;
    height: 100%;
  }
</style>
