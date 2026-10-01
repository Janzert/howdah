<script lang="ts">
  import { SQ, TRAPS, squareName, squareXY } from '../geometry';
  import type { Coordinates } from '../settings.svelte';
  import type { Theme } from '../theme';

  let { theme, flipped, coordinates }: { theme: Theme; flipped: boolean; coordinates: Coordinates } = $props();

  const squares = Array.from({ length: 64 }, (_, i) => i);

  // Image boards are drawn so the playing grid lands on 0..800.
  const imageRect = $derived.by(() => {
    if (theme.board.kind !== 'image') return null;
    const { width, height, grid } = theme.board;
    const sx = 800 / grid.width;
    const sy = 800 / grid.height;
    return { x: -grid.x * sx, y: -grid.y * sy, w: width * sx, h: height * sy };
  });
</script>

{#if theme.board.kind === 'image' && imageRect}
  <!-- The classic boards are symmetric, so flipping needs no image change. -->
  <image
    href={theme.board.src}
    x={imageRect.x}
    y={imageRect.y}
    width={imageRect.w}
    height={imageRect.h}
    preserveAspectRatio="none"
  />
{:else if theme.board.kind === 'procedural'}
  {@const b = theme.board}
  {#each squares as sq (sq)}
    {@const p = squareXY(sq, flipped)}
    <rect
      x={p.x}
      y={p.y}
      width={SQ}
      height={SQ}
      fill={TRAPS.includes(sq) ? b.trap : ((sq % 8) + Math.floor(sq / 8)) % 2 === 0 ? b.dark : b.light}
      stroke={b.line ?? 'none'}
      stroke-width="1"
    />
  {/each}
{/if}

{#if coordinates === 'all'}
  {#each [0, 1, 2, 3, 4, 5, 6, 7] as i (i)}
    <!-- Files along the bottom edge, ranks along the left edge. -->
    <text class="coord" x={i * SQ + 94} y={794} text-anchor="end" fill={theme.ui.coord}>
      {'abcdefgh'[flipped ? 7 - i : i]}
    </text>
    <text class="coord" x={5} y={i * SQ + 18} fill={theme.ui.coord}>{flipped ? i + 1 : 8 - i}</text>
  {/each}
{:else if coordinates === 'traps'}
  {#each TRAPS as sq (sq)}
    {@const p = squareXY(sq, flipped)}
    <text class="coord trap" x={p.x + 6} y={p.y + 21} fill={theme.ui.coord}>{squareName(sq)}</text>
  {/each}
{/if}

<style>
  .coord {
    font: 600 15px system-ui, sans-serif;
    pointer-events: none;
    user-select: none;
  }
  /* Trap squares are often dark; a light halo keeps the label readable. */
  .trap {
    font-size: 18px;
    stroke: rgba(255, 255, 255, 0.55);
    stroke-width: 3px;
    paint-order: stroke;
  }
</style>
