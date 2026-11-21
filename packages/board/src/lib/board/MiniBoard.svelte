<script lang="ts">
  // A small read-only board for previews: the board and pieces of a
  // position, no input. Given a `model`, it shows that model's pieces
  // instead, so its moves slide and its captures fade as on the board.
  import type { PositionView } from '../types';
  import { squareXY } from '../geometry';
  import { viewBox, type Theme } from '../theme';
  import type { BoardModel } from './boardModel.svelte';
  import BoardSurface from './BoardSurface.svelte';
  import PieceGlyph from './PieceGlyph.svelte';

  let {
    position,
    theme,
    flipped,
    model,
  }: { position: PositionView; theme: Theme; flipped: boolean; model?: BoardModel } = $props();

  const translate = (sq: number) => {
    const at = squareXY(sq, flipped);
    return `transform: translate(${at.x}px, ${at.y}px)`;
  };
</script>

<svg
  viewBox={viewBox(theme)}
  preserveAspectRatio="xMidYMid meet"
  aria-hidden="true"
  style:--step-ms={model ? `${model.stepMs}ms` : undefined}
  style:--fade-ms={model ? `${model.fadeMs}ms` : undefined}
>
  <BoardSurface {theme} {flipped} coordinates="none" />
  {#if model}
    {#each model.pieces as p (p.id)}
      <g class="piece" class:instant={p.instant} style={translate(p.square)}>
        <g class="glyph" class:fade-out={p.fading === 'out'} class:fade-in={p.fading === 'in'}>
          <PieceGlyph piece={p.piece} {theme} />
        </g>
      </g>
    {/each}
  {:else}
    {#each position.pieces as p (p.id)}
      {@const at = squareXY(p.square, flipped)}
      <g transform="translate({at.x}, {at.y})">
        <PieceGlyph piece={p.piece} {theme} />
      </g>
    {/each}
  {/if}
</svg>

<style>
  svg {
    display: block;
    width: 100%;
    height: 100%;
  }
  /* As on the board (Board.svelte). */
  .piece {
    transition: transform var(--step-ms, 220ms) ease-in-out;
  }
  .piece.instant {
    transition: none;
  }
  .glyph {
    transform-box: fill-box;
    transform-origin: center;
    transition:
      opacity var(--fade-ms, 260ms) ease-in,
      transform var(--fade-ms, 260ms) ease-in;
  }
  .glyph.fade-out {
    opacity: 0;
    transform: scale(0.2);
  }
  .glyph.fade-in {
    animation: appear var(--fade-ms, 260ms) ease-out;
  }
  @keyframes appear {
    from {
      opacity: 0;
      transform: scale(0.2);
    }
  }
</style>
