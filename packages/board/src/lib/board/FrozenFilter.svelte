<!-- The frozen-piece marker as an SVG filter: the piece frosted over (colors
     cooled toward ice, a light rim inside its edge, faint specks) inside a
     soft icy glow and outline. Shapes come from the piece's alpha, so it
     fits any theme's pieces. Lengths are in board units (a square is 100). -->
<script lang="ts">
  import { FROZEN_DEFAULTS, type FrozenSpec } from '../theme';

  let { id, spec }: { id: string; spec: FrozenSpec | undefined } = $props();

  const f = $derived({ ...FROZEN_DEFAULTS, ...spec });
</script>

<filter {id} x="-15%" y="-15%" width="130%" height="130%" color-interpolation-filters="sRGB">
  <!-- Colors cooled toward ice, mixed back with the original. -->
  <feColorMatrix
    in="SourceGraphic"
    type="matrix"
    values="0.35 0.35 0.2 0 0.08  0.35 0.4 0.25 0 0.14  0.3 0.4 0.4 0 0.26  0 0 0 1 0"
    result="cold"
  />
  <feComposite in="SourceGraphic" in2="cold" operator="arithmetic" k2={1 - f.cool} k3={f.cool} result="cooled" />
  <!-- A light rim just inside the edge. -->
  <feMorphology in="SourceAlpha" operator="erode" radius="3" result="inner" />
  <feComposite in="SourceAlpha" in2="inner" operator="out" result="rim" />
  <feGaussianBlur in="rim" stdDeviation="1.6" result="rimBlur" />
  <feComposite in="rimBlur" in2="SourceAlpha" operator="in" result="rimIn" />
  <feFlood flood-color="#eef8ff" flood-opacity={f.rim} />
  <feComposite in2="rimIn" operator="in" result="rimColor" />
  <!-- Frost specks from noise, kept inside the piece. -->
  <feTurbulence type="fractalNoise" baseFrequency="0.11" numOctaves="2" seed="7" result="noise" />
  <feColorMatrix in="noise" type="matrix" values="0 0 0 0 1  0 0 0 0 1  0 0 0 0 1  4 0 0 0 -2.3" result="specks" />
  <feComposite in="specks" in2="SourceAlpha" operator="in" result="specksIn" />
  <feComponentTransfer in="specksIn" result="frost">
    <feFuncA type="linear" slope={f.specks} />
  </feComponentTransfer>
  <!-- The glow and outline around the piece. -->
  <feMorphology in="SourceAlpha" operator="dilate" radius="4" result="fat" />
  <feGaussianBlur in="fat" stdDeviation="2.5" result="glowShape" />
  <feFlood flood-color={f.glow} />
  <feComposite in2="glowShape" operator="in" result="glow" />
  <feMorphology in="SourceAlpha" operator="dilate" radius="2" result="ring" />
  <feFlood flood-color={f.outline} />
  <feComposite in2="ring" operator="in" result="outline" />
  <feMerge>
    <feMergeNode in="glow" />
    <feMergeNode in="outline" />
    <feMergeNode in="cooled" />
    <feMergeNode in="frost" />
    <feMergeNode in="rimColor" />
  </feMerge>
</filter>
