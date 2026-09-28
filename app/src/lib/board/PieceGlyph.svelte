<script lang="ts">
  import type { Piece } from '../bindings/Piece';
  import { SQ } from '../geometry';
  import { pieceLetter, type Theme } from '../theme';

  let { piece, theme }: { piece: Piece; theme: Theme } = $props();

  const letter = $derived(pieceLetter(piece));
</script>

{#if theme.pieces.kind === 'image'}
  {@const size = SQ * (theme.pieces.scale ?? 1)}
  <image
    href={theme.pieces.srcs[letter]}
    x={(SQ - size) / 2}
    y={(SQ - size) / 2}
    width={size}
    height={size}
  />
{:else}
  {@const c = piece.color === 'gold' ? theme.pieces.gold : theme.pieces.silver}
  <circle cx={SQ / 2} cy={SQ / 2} r={SQ * 0.4} fill={c.fill} stroke={c.stroke} stroke-width="4" />
  <text
    x={SQ / 2}
    y={SQ / 2}
    text-anchor="middle"
    dominant-baseline="central"
    fill={c.text}
    font-size="44"
    font-weight="700"
    font-family="system-ui, sans-serif">{letter.toUpperCase()}</text
  >
{/if}
