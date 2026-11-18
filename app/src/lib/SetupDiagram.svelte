<script lang="ts">
  // One side's setup as its two home rows, seen from gold's side (files a
  // to h left to right): gold's front row above its back row, silver's
  // back row above its front row, as they stand on the board.
  import PieceGlyph from '@howdah/board/board/PieceGlyph.svelte';
  import { SQ } from '@howdah/board/geometry';
  import { findTheme } from '@howdah/board/theme';
  import type { Color } from './bindings/Color';
  import { setupPieces, setupWords } from './library';
  import { settings } from './settings.svelte';

  interface Props {
    /** A setup key (`setupPieces`). */
    setup: string;
    color: Color;
    /** Square size in pixels. */
    size?: number;
  }
  let { setup, color, size = 24 }: Props = $props();

  const theme = $derived(findTheme(settings.theme));
  const pieces = $derived(setupPieces(setup, color));
  const label = $derived(`${color} setup: ${setupWords(setup, color)}`);
  /** Row 0 is drawn on top. */
  const row = (back: boolean) => (color === 'gold' ? (back ? 1 : 0) : back ? 0 : 1);
</script>

<svg class="setup" viewBox="0 0 {8 * SQ} {2 * SQ}" width={8 * size} height={2 * size} role="img" aria-label={label}>
  <title>{label}</title>
  {#each [0, 1] as r (r)}
    {#each Array.from({ length: 8 }, (_, f) => f) as f (f)}
      <rect class:dark={(f + r) % 2 === 1} x={f * SQ} y={r * SQ} width={SQ} height={SQ} />
    {/each}
  {/each}
  {#each pieces as p (`${p.file}${p.back}`)}
    <g transform="translate({p.file * SQ}, {row(p.back) * SQ})">
      <PieceGlyph piece={{ color, kind: p.kind }} {theme} />
    </g>
  {/each}
</svg>

<style>
  svg {
    display: block;
    flex: none;
    border-radius: 2px;
  }
  rect {
    fill: var(--setup-light, #e9dfc8);
  }
  rect.dark {
    fill: var(--setup-dark, #d8caa9);
  }
</style>
