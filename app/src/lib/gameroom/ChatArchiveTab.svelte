<script lang="ts">
  // The gameroom chat room's archive as kept here: a calendar of the days
  // with what the local archive holds of each (messages and people; all of
  // the day, or part), and the day open, read in the room's formatting.
  // Opening a day asks arimaa.com only for what isn't kept yet; no login
  // is needed. What the user sees in the room is kept too.
  import Icon from '../Icon.svelte';
  import type { ChatDaySummary } from '../bindings/ChatDaySummary';
  import ChatLine from './ChatLine.svelte';
  import { addDays, chatArchive, isoDate } from './chatArchive.svelte';

  $effect(() => chatArchive.start());

  const today = isoDate(new Date());
  const monthName = $derived(
    new Date(chatArchive.year, chatArchive.month - 1, 1).toLocaleDateString(undefined, {
      month: 'long',
      year: 'numeric',
    }),
  );
  const monthValue = $derived(`${chatArchive.year}-${String(chatArchive.month).padStart(2, '0')}`);
  /** Blank cells before the 1st, for a week from Sunday. */
  const lead = $derived(new Date(chatArchive.year, chatArchive.month - 1, 1).getDay());
  const weekdays = Array.from({ length: 7 }, (_, i) =>
    new Date(2026, 1, 1 + i).toLocaleDateString(undefined, { weekday: 'narrow' }),
  );

  const clock = (unix: number) =>
    new Date(unix * 1000).toLocaleTimeString(undefined, { hour: '2-digit', minute: '2-digit', second: '2-digit' });
  const shortClock = (unix: number) =>
    new Date(unix * 1000).toLocaleTimeString(undefined, { hour: '2-digit', minute: '2-digit' });

  /** The stretches kept, `6:54 PM–midnight`. */
  function spans(d: ChatDaySummary): string {
    const end = new Date(d.date + 'T00:00');
    end.setDate(end.getDate() + 1);
    const at = (t: number) => (t >= end.getTime() / 1000 ? 'midnight' : shortClock(t));
    const start = new Date(d.date + 'T00:00').getTime() / 1000;
    if (d.covered.length === 0) return 'only a few lines kept';
    return 'kept ' + d.covered.map(([a, b]) => `${a <= start ? 'midnight' : at(a)}–${at(b)}`).join(', ');
  }

  function describe(d: ChatDaySummary): string {
    if (d.future) return 'Not yet';
    if (d.coverage === 'none') return 'Not kept here yet';
    const counts = `${d.messages} message${d.messages === 1 ? '' : 's'}, ${d.people} ${d.people === 1 ? 'person' : 'people'}`;
    if (d.coverage === 'full') return d.today ? `${counts} so far` : counts;
    return `${counts} (part of the day: ${spans(d)})`;
  }

  const day = $derived(chatArchive.day);
  const s = $derived(day?.summary);
  const longDate = $derived(
    chatArchive.selected
      ? new Date(chatArchive.selected + 'T00:00').toLocaleDateString(undefined, {
          weekday: 'long',
          year: 'numeric',
          month: 'long',
          day: 'numeric',
        })
      : '',
  );
</script>

<div class="archive">
  <section class="card calendar" aria-labelledby="archive-month">
    <div class="month-head">
      <button class="icon" onclick={() => chatArchive.shiftMonth(-1)} aria-label="Previous month" title="Previous month"
        ><Icon name="prev" size={14} /></button
      >
      <h2 id="archive-month">{monthName}</h2>
      <button class="icon" onclick={() => chatArchive.shiftMonth(1)} aria-label="Next month" title="Next month"
        ><Icon name="next" size={14} /></button
      >
      <span class="spacer"></span>
      <input
        type="month"
        value={monthValue}
        max={today.slice(0, 7)}
        aria-label="Go to month"
        onchange={(e) => {
          const [y, m] = e.currentTarget.value.split('-').map(Number);
          if (y && m) chatArchive.showMonth(y, m);
        }}
      />
    </div>
    <div class="grid" role="grid" aria-label="Days of {monthName}">
      {#each weekdays as w, i (i)}<span class="weekday" aria-hidden="true">{w}</span>{/each}
      {#each { length: lead } as _, i (i)}<span></span>{/each}
      {#each chatArchive.days?.days ?? [] as d (d.date)}
        <button
          class="day {d.coverage}"
          class:today={d.today}
          class:on={d.date === chatArchive.selected}
          disabled={d.future}
          onclick={() => chatArchive.open(d.date)}
          title="{d.date}: {describe(d)}"
          aria-label="{d.date}: {describe(d)}"
          aria-current={d.date === chatArchive.selected ? 'date' : undefined}
        >
          <span class="num">{Number(d.date.slice(8))}</span>
          {#if d.coverage !== 'none'}
            <span class="counts"><strong>{d.messages}</strong> · {d.people}</span>
          {/if}
        </button>
      {/each}
    </div>
    <p class="legend">
      <span class="key full"></span> kept <span class="key partial"></span> part kept · messages · people. Opening a day
      gets what isn't kept yet from arimaa.com; the days you're in the chat room are kept as you go.
    </p>
  </section>

  <section class="card day-view" aria-labelledby="archive-day">
    {#if !chatArchive.selected}
      <p class="empty">Pick a day to read it.</p>
    {:else}
      <div class="day-head">
        <button
          class="icon"
          onclick={() => chatArchive.open(addDays(chatArchive.selected!, -1))}
          aria-label="Previous day"
          title="Previous day"><Icon name="prev" size={14} /></button
        >
        <h2 id="archive-day">{longDate}</h2>
        <button
          class="icon"
          onclick={() => chatArchive.open(addDays(chatArchive.selected!, 1))}
          disabled={chatArchive.selected >= today}
          aria-label="Next day"
          title="Next day"><Icon name="next" size={14} /></button
        >
        <span class="spacer"></span>
        {#if s && !s.future && (s.coverage !== 'full' || s.today)}
          <button onclick={() => chatArchive.fetch()} disabled={chatArchive.fetching}
            >{s.today ? 'Update' : 'Get the rest'}</button
          >
        {/if}
      </div>
      {#if s}<p class="muted">{describe(s)}</p>{/if}
      {#if chatArchive.fetching}<p class="muted" role="status">Getting it from arimaa.com…</p>{/if}
      {#if chatArchive.error}<p class="error" role="alert">{chatArchive.error}</p>{/if}
      <div class="log" role="log" aria-label="Chat on {longDate}">
        {#each day?.events ?? [] as e (e.id)}
          <ChatLine event={e} time={clock} />
        {:else}
          {#if !chatArchive.fetching}<p class="empty">Nothing kept for this day.</p>{/if}
        {/each}
      </div>
    {/if}
  </section>
</div>

<style>
  .archive {
    display: grid;
    grid-template-columns: minmax(280px, 340px) 1fr;
    gap: 14px;
    align-items: start;
  }
  @media (max-width: 760px) {
    .archive {
      grid-template-columns: 1fr;
    }
  }
  .month-head,
  .day-head {
    display: flex;
    align-items: center;
    gap: 6px;
    margin-bottom: 10px;
  }
  h2 {
    margin: 0;
    font-size: 16px;
  }
  button.icon {
    display: flex;
    padding: 4px 6px;
  }
  .month-head input {
    font-size: 12px;
    max-width: 140px;
  }
  .grid {
    display: grid;
    grid-template-columns: repeat(7, 1fr);
    gap: 3px;
  }
  .weekday {
    text-align: center;
    font-size: 11px;
    color: var(--muted);
    padding-bottom: 2px;
  }
  .day {
    display: flex;
    flex-direction: column;
    align-items: center;
    justify-content: flex-start;
    gap: 1px;
    min-height: 42px;
    padding: 3px 2px;
    border: 1px solid transparent;
    border-radius: 6px;
    background: none;
    font-size: 13px;
  }
  .day:disabled {
    opacity: 0.35;
  }
  .day.full {
    background: color-mix(in srgb, var(--accent) 22%, transparent);
  }
  .day.partial {
    background: linear-gradient(
      135deg,
      color-mix(in srgb, var(--accent) 22%, transparent) 50%,
      transparent 50%
    );
    border: 1px dashed color-mix(in srgb, var(--accent) 60%, transparent);
  }
  .day.today .num {
    font-weight: 700;
    color: var(--accent);
  }
  .day.on {
    outline: 2px solid var(--accent);
    outline-offset: -1px;
  }
  .counts {
    font-size: 10px;
    color: var(--muted);
    white-space: nowrap;
  }
  .counts strong {
    color: var(--text);
    font-weight: 600;
  }
  .legend {
    margin: 10px 0 0;
    font-size: 12px;
    color: var(--muted);
  }
  .key {
    display: inline-block;
    width: 12px;
    height: 12px;
    border-radius: 3px;
    vertical-align: -2px;
  }
  .key.full {
    background: color-mix(in srgb, var(--accent) 22%, transparent);
  }
  .key.partial {
    background: linear-gradient(135deg, color-mix(in srgb, var(--accent) 22%, transparent) 50%, transparent 50%);
    border: 1px dashed color-mix(in srgb, var(--accent) 60%, transparent);
  }
  .day-view {
    min-width: 0;
  }
  .muted {
    margin: 0 0 6px;
    font-size: 13px;
    color: var(--muted);
  }
  .error {
    margin: 0 0 6px;
    font-size: 13px;
    color: var(--warn);
  }
  .log {
    max-height: calc(100vh - 240px);
    min-height: 200px;
    overflow-y: auto;
    padding: 8px 10px;
    border: 1px solid var(--border);
    border-radius: 8px;
    background: var(--panel);
    font-size: 14px;
    line-height: 1.45;
    overflow-wrap: anywhere;
  }
</style>
