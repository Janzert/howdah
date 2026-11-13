<script lang="ts">
  // One arimaa.com player's page: who they are and their rating (from
  // their player page), their rating over time, and their games or their
  // record by opponent, with the games against each. Opened from the
  // online players; an opponent in the record opens their own panel.
  import type { RecordLineView } from '../bindings/RecordLineView';
  import type { TallyView } from '../bindings/TallyView';
  import PastGameRow from './PastGameRow.svelte';
  import { type PlayerPage, type RecordFilter, showPlayerPage } from './player.svelte';
  import RatingChart from './RatingChart.svelte';
  import { gameroom } from './store.svelte';

  let { page }: { page: PlayerPage } = $props();

  const p = $derived(page.profile.value);
  const username = $derived(p?.username ?? page.player.username);

  /** Their game being played now, to watch. */
  const live = $derived(gameroom.games?.live.find((g) => g.gold === username || g.silver === username) ?? null);

  /** The time where they are, updated each minute. */
  let now = $state(Date.now());
  $effect(() => {
    const timer = setInterval(() => (now = Date.now()), 30_000);
    return () => clearInterval(timer);
  });
  const localTime = $derived.by(() => {
    if (p?.utcOffset == null) return null;
    const d = new Date(now + p.utcOffset * 1000);
    const time = d.toLocaleTimeString(undefined, { hour: 'numeric', minute: '2-digit', timeZone: 'UTC' });
    const h = p.utcOffset / 3600;
    const zone = h === 0 ? 'UTC' : `UTC${h > 0 ? '+' : '−'}${Math.abs(h)}`;
    return `${time} (${zone})`;
  });

  const ratingNote = $derived(
    [p?.ratingUncertainty != null ? `RU ${p.ratingUncertainty}` : null, p?.provisional ? 'provisional' : null]
      .filter(Boolean)
      .join(' · '),
  );
  const sides = $derived(
    p?.gamesGold != null && p?.gamesSilver != null ? `${p.gamesGold} as gold, ${p.gamesSilver} as silver` : null,
  );
  const plural = (n: number, one: string, many = `${one}s`) => `${n} ${n === 1 ? one : many}`;
  const streakText = (s: number) => (s > 0 ? plural(s, 'win') : s < 0 ? plural(-s, 'loss', 'losses') : 'none');
  const pct = (t: TallyView) => (t.games ? `${Math.round((100 * (t.wins + t.draws / 2)) / t.games)}%` : '–');
  const wl = (t: TallyView) => (t.games ? `${t.wins}–${t.losses}${t.draws ? `–${t.draws}` : ''}` : '–');

  /** Opponents shown in the record before "Show all". */
  const RECORD_SHOWN = 30;
  let recordAll = $state(false);
  let recordWords = $state('');
  const recordRows = $derived.by((): RecordLineView[] => {
    const rows = page.record.value?.opponents ?? [];
    const words = recordWords.trim().toLowerCase();
    return words ? rows.filter((r) => r.opponent?.toLowerCase().includes(words)) : rows;
  });
  const recordShown = $derived(recordAll || recordWords.trim() ? recordRows : recordRows.slice(0, RECORD_SHOWN));

  const ratedChoices: [boolean | null, string][] = [
    [null, 'All games'],
    [true, 'Rated'],
    [false, 'Unrated'],
  ];
  const botChoices: [boolean | null, string][] = [
    [null, 'All opponents'],
    [false, 'Humans'],
    [true, 'Bots'],
  ];
  function setFilter(change: Partial<RecordFilter>) {
    page.setRecordFilter({ ...page.recordFilter, ...change });
  }
</script>

<section class="card" aria-labelledby="pp-name">
  <div class="card-head">
    <h2 id="pp-name">{username}</h2>
    {#if p?.bot}<span class="tag">bot</span>{/if}
    {#if p?.online}<span class="tag online">online</span>{/if}
    <span class="spacer"></span>
    {#if live}
      <button onclick={() => gameroom.open(live.gid)} disabled={gameroom.busy} title="Watch game {live.gid}"
        >Watch</button
      >
    {/if}
    {#if gameroom.invitable.has(username)}
      <button
        onclick={() => (gameroom.newGame = { opponent: username })}
        disabled={gameroom.busy}
        title="Invite {username} to a game">Invite</button
      >
    {/if}
    <button class="subtle" onclick={() => page.reload()} title="Fetch the page again">Refresh</button>
  </div>
  {#if page.profile.loading}
    <p class="empty">Loading…</p>
  {:else if page.profile.error}
    <p class="error">{page.profile.error}</p>
  {:else if p}
    <div class="summary">
      <div class="rating-box">
        <span class="big">{p.rating ?? '–'}</span>
        {#if ratingNote}<span class="meta">{ratingNote}</span>{/if}
      </div>
      <dl class="facts">
        {#if p.name}<dt>Name</dt>
          <dd>{p.name}</dd>{/if}
        {#if p.country}<dt>From</dt>
          <dd>{p.country}</dd>{/if}
        {#if localTime}<dt>Their time</dt>
          <dd>{localTime}</dd>{/if}
        {#if p.lastSeen && !p.online}<dt>Last seen</dt>
          <dd>{p.lastSeen}</dd>{/if}
        {#if p.joined}<dt>Joined</dt>
          <dd>
            {p.joined}{#if p.number != null}<span class="meta">{` · player #${p.number}`}</span>{/if}
          </dd>{/if}
      </dl>
      <dl class="facts">
        {#if p.games != null}<dt>Games</dt>
          <dd>
            {p.games}{#if sides}<span class="meta">{` · ${sides}`}</span>{/if}
          </dd>{/if}
        {#if p.wins != null && p.losses != null}<dt>Won</dt>
          <dd>
            {p.wins}–{p.losses}{#if p.draws}–{p.draws}{/if}{#if p.games}<span class="meta"
                >{` · ${Math.round((100 * p.wins) / p.games)}%`}</span
              >{/if}
          </dd>{/if}
        {#if p.streak != null}<dt>Streak</dt>
          <dd>{streakText(p.streak)}</dd>{/if}
      </dl>
    </div>
  {/if}
</section>

<section class="card" aria-labelledby="pp-rating">
  <div class="card-head"><h2 id="pp-rating">Rating</h2></div>
  {#if page.history.loading || (!page.history.value && !page.history.error)}
    <p class="empty">Loading…</p>
  {:else if page.history.error}
    <p class="error">{page.history.error}</p>
  {:else if page.history.value?.length === 0}
    <p class="empty">No rated games.</p>
  {:else if page.history.value}
    <RatingChart
      points={page.history.value}
      current={p?.rating ?? null}
      onOpen={(gid) => gameroom.open(gid)}
      label="{username}'s rating over time"
    />
  {/if}
</section>

<section class="card" aria-label="Games and record">
  <div class="card-head">
    <div class="tabs" role="tablist">
      <button
        role="tab"
        aria-selected={page.tab === 'games'}
        class:on={page.tab === 'games'}
        onclick={() => page.showGames()}>Games</button
      >
      <button
        role="tab"
        aria-selected={page.tab === 'record'}
        class:on={page.tab === 'record'}
        onclick={() => page.showRecord()}>Record by opponent</button
      >
    </div>
  </div>

  {#if page.tab === 'games'}
    {@const part = page.games}
    {#if part.value}
      {#if part.value.games.length === 0}
        <p class="empty">No finished games.</p>
      {:else}
        <ul class="list" aria-label="{username}'s games">
          {#each part.value.games as g (g.gid)}
            <PastGameRow game={g} />
          {/each}
        </ul>
      {/if}
      {#if part.value.next != null}
        <button class="subtle more" onclick={() => page.olderGames()} disabled={part.loading}
          >{part.loading ? 'Loading…' : 'Older games'}</button
        >
      {/if}
    {:else if part.loading || !part.error}
      <p class="empty">Loading…</p>
    {/if}
    {#if part.error}<p class="error">{part.error}</p>{/if}
  {:else}
    <div class="filters">
      <div class="choices" role="group" aria-label="Rated or unrated">
        {#each ratedChoices as [v, label] (label)}
          <button
            class="link"
            class:on={page.recordFilter.rated === v}
            aria-pressed={page.recordFilter.rated === v}
            onclick={() => setFilter({ rated: v })}>{label}</button
          >
        {/each}
      </div>
      <div class="choices" role="group" aria-label="Opponents">
        {#each botChoices as [v, label] (label)}
          <button
            class="link"
            class:on={page.recordFilter.bots === v}
            aria-pressed={page.recordFilter.bots === v}
            onclick={() => setFilter({ bots: v })}>{label}</button
          >
        {/each}
      </div>
      <span class="spacer"></span>
      <input bind:value={recordWords} placeholder="Filter by name" aria-label="Filter opponents" autocomplete="off" />
    </div>
    {@const part = page.record}
    {#if part.loading}
      <p class="empty">Loading…</p>
    {:else if part.error}
      <p class="error">{part.error}</p>
    {:else if part.value}
      {@const totals = part.value.totals}
      {#if !totals || totals.all.games === 0}
        <p class="empty">No games.</p>
      {:else}
        <table class="record">
          <thead>
            <tr>
              <th scope="col">Opponent</th>
              <th scope="col" class="num">Games</th>
              <th scope="col" class="num">Won–lost</th>
              <th scope="col" class="num">Score</th>
              <th scope="col" class="num side"><span class="dot gold"></span> As gold</th>
              <th scope="col" class="num side"><span class="dot silver"></span> As silver</th>
              <th scope="col"><span class="sr-only">Games against</span></th>
            </tr>
          </thead>
          <tbody>
            <tr class="totals">
              <th scope="row">{plural(part.value.opponents.length, 'opponent')}</th>
              <td class="num">{totals.all.games}</td>
              <td class="num">{wl(totals.all)}</td>
              <td class="num">{pct(totals.all)}</td>
              <td class="num side">{wl(totals.gold)}</td>
              <td class="num side">{wl(totals.silver)}</td>
              <td></td>
            </tr>
            {#each recordShown as r (r.opponentId)}
              {@const open = page.againstId === r.opponentId}
              <tr class:open>
                <th scope="row">
                  <button
                    class="link"
                    onclick={() => r.opponentId && showPlayerPage({ id: r.opponentId, username: r.opponent ?? '' })}
                    title="{r.opponent}'s page">{r.opponent}</button
                  >
                </th>
                <td class="num">{r.all.games}</td>
                <td class="num">{wl(r.all)}</td>
                <td class="num">{pct(r.all)}</td>
                <td class="num side">{wl(r.gold)}</td>
                <td class="num side">{wl(r.silver)}</td>
                <td>
                  <button
                    class="subtle"
                    aria-expanded={open}
                    onclick={() => r.opponentId && page.toggleAgainst(r.opponentId)}
                    title="{username}'s games against {r.opponent}">{open ? 'Hide' : 'Games'}</button
                  >
                </td>
              </tr>
              {#if open}
                <tr class="against">
                  <td colspan="7">
                    {#if page.against.loading}
                      <p class="empty">Loading…</p>
                    {:else if page.against.error}
                      <p class="error">{page.against.error}</p>
                    {:else if page.against.value}
                      <ul class="list" aria-label="Games against {r.opponent}">
                        {#each page.against.value as g (g.gid)}
                          <PastGameRow game={g} />
                        {/each}
                      </ul>
                    {/if}
                  </td>
                </tr>
              {/if}
            {/each}
          </tbody>
        </table>
        {#if recordShown.length < recordRows.length}
          <button class="subtle more" onclick={() => (recordAll = true)}>Show all {recordRows.length} opponents</button>
        {/if}
      {/if}
    {/if}
  {/if}
</section>

<style>
  .card + .card {
    margin-top: 12px;
  }
  .card-head h2 {
    margin: 0;
  }
  .tag.online {
    color: var(--clock-ok);
    border-color: currentColor;
  }
  .summary {
    display: flex;
    flex-wrap: wrap;
    gap: 12px 32px;
    align-items: flex-start;
  }
  .rating-box {
    display: flex;
    flex-direction: column;
    align-items: flex-start;
    min-width: 90px;
  }
  .big {
    font-size: 30px;
    font-weight: 600;
    line-height: 1.1;
    font-variant-numeric: tabular-nums;
  }
  .facts {
    display: grid;
    grid-template-columns: auto 1fr;
    gap: 3px 12px;
    margin: 0;
    font-size: 13px;
  }
  .facts dt {
    color: var(--muted);
  }
  .facts dd {
    margin: 0;
  }
  .tabs {
    display: flex;
    gap: 4px;
  }
  .tabs button {
    background: none;
    border: none;
    border-bottom: 2px solid transparent;
    border-radius: 0;
    padding: 4px 8px;
    font-weight: 600;
    color: var(--muted);
  }
  .tabs button.on {
    color: var(--text);
    border-bottom-color: var(--accent);
  }
  .filters {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    gap: 6px 18px;
    margin-bottom: 8px;
    font-size: 13px;
  }
  .choices {
    display: flex;
    gap: 10px;
  }
  .choices .on {
    font-weight: 600;
    text-decoration: underline;
  }
  .record {
    width: 100%;
    border-collapse: collapse;
    font-size: 13px;
  }
  .record th,
  .record td {
    padding: 4px 6px;
    text-align: left;
    border-bottom: 1px solid var(--border);
  }
  .record thead th {
    font-size: 12px;
    color: var(--muted);
    font-weight: 600;
  }
  .record .num {
    text-align: right;
    font-variant-numeric: tabular-nums;
    white-space: nowrap;
  }
  .record tbody th {
    font-weight: normal;
  }
  .record .totals {
    font-weight: 600;
  }
  .record .totals th {
    font-weight: 600;
  }
  .record tr.open > * {
    background: var(--hover);
  }
  .record .against > td {
    padding: 0 0 8px 16px;
    background: var(--hover);
  }
  .record .against .list {
    background: var(--panel);
  }
  .record td:last-child {
    text-align: right;
    width: 1%;
  }
  @media (max-width: 640px) {
    .record .side {
      display: none;
    }
  }
  .more {
    margin-top: 8px;
  }
  .sr-only {
    position: absolute;
    width: 1px;
    height: 1px;
    overflow: hidden;
    clip: rect(0 0 0 0);
  }
</style>
