<script lang="ts">
  // Random / Gold / Silver as one segmented control (radio buttons, so the
  // arrow keys move between them), for the side the user plays.
  import type { Color } from './bindings/Color';

  interface Props {
    value: Color | 'random';
    /** The radio group's name, unique on the page. */
    name: string;
    /** The id of the label naming the control. */
    labelledby: string;
  }
  let { value = $bindable(), name, labelledby }: Props = $props();

  const choices: [Color | 'random', string][] = [
    ['random', 'Random'],
    ['gold', 'Gold'],
    ['silver', 'Silver'],
  ];
</script>

<div class="segmented" role="radiogroup" aria-labelledby={labelledby}>
  {#each choices as [choice, label] (choice)}
    <label class:on={value === choice}>
      <input type="radio" {name} value={choice} bind:group={value} />
      {#if choice !== 'random'}<span class="dot {choice}"></span>{/if}{label}
    </label>
  {/each}
</div>

<style>
  .segmented {
    display: inline-flex;
    justify-self: start;
    align-self: flex-start;
    border: 1px solid var(--border);
    border-radius: 6px;
    overflow: hidden;
    font-size: 13px;
  }
  label {
    display: flex;
    align-items: center;
    gap: 5px;
    padding: 4px 12px;
    cursor: pointer;
  }
  label + label {
    border-left: 1px solid var(--border);
  }
  label.on {
    background: var(--accent);
    color: var(--accent-text);
  }
  input {
    position: absolute;
    opacity: 0;
    pointer-events: none;
  }
  label:focus-within {
    outline: 2px solid var(--accent);
    outline-offset: -2px;
  }
  .dot {
    width: 9px;
    height: 9px;
    border-radius: 50%;
    border: 1px solid rgba(0, 0, 0, 0.4);
  }
  .dot.gold {
    background: #e3b23c;
  }
  .dot.silver {
    background: #c9ced6;
  }
</style>
