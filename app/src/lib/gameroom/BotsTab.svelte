<script lang="ts">
  // The server's bots (4steps's bot launcher): the official bot ladder,
  // with the user's place on it, or every bot; the one chosen, from its
  // page, with Start. The lists are two page fetches, made when the tab
  // first opens.
  import { onMount } from 'svelte';
  import type { Color } from '../bindings/Color';
  import type { ServerBotView } from '../bindings/ServerBotView';
  import SidePicker from '../SidePicker.svelte';
  import { botRecord } from './format';
  import { gameroom, pref, savePref } from './store.svelte';

  let botSet = $state<'ladder' | 'all'>(pref('gameroom.botSet') === 'all' ? 'all' : 'ladder');
  let filter = $state('');
  let order = $state<'ladder' | 'rating' | 'name'>('ladder');
  /** The user's side against it. */
  let userSide = $state<Color | 'random'>((pref('gameroom.botSide') as Color | 'random') || 'random');
  /** Whether to sit as soon as the bot's game opens (4steps's "Join game
   * on creation"), and whether to keep a rated bot game rated (off: sit in
   * the gameroom's unrated mode). */
  let join = $state(pref('gameroom.botJoin') !== '0');
  let keepRated = $state(pref('gameroom.botRated') !== '0');

  onMount(() => {
    if (gameroom.bots == null && !gameroom.busy) void gameroom.loadBots();
  });

  const bots = $derived(gameroom.bots);
  const chosen = $derived(gameroom.bot);

  /** The rows of the list: bots, and in the ladder's own order the user's
   * place on it. */
  type BotRow = { bot: ServerBotView } | { me: true };
  const ladderShown = $derived(botSet === 'ladder' && (bots?.ladder.length ?? 0) > 0);
  const rows = $derived.by((): BotRow[] => {
    if (!bots) return [];
    const words = filter.trim().toLowerCase();
    const byName = new Map(bots.bots.map((b) => [b.name, b]));
    const set = ladderShown ? bots.ladder.flatMap((n) => byName.get(n) ?? []) : [...bots.bots];
    const list = set.filter((b) => !words || b.name.toLowerCase().includes(words));
    if (order === 'rating') list.sort((a, b) => (b.rating ?? 0) - (a.rating ?? 0));
    else if (order === 'name') list.sort((a, b) => a.name.localeCompare(b.name));
    const out: BotRow[] = list.map((bot) => ({ bot }));
    if (ladderShown && order === 'ladder' && !words) {
      const above = bots.ladder.slice(0, bots.ladderAbove);
      out.splice(list.filter((b) => above.includes(b.name)).length, 0, { me: true });
    }
    return out;
  });

  function start(e: SubmitEvent) {
    e.preventDefault();
    savePref('gameroom.botSide', userSide);
    savePref('gameroom.botJoin', join ? '1' : '0');
    savePref('gameroom.botRated', keepRated ? '1' : '0');
    void gameroom.startBot(userSide, join, !keepRated);
  }
</script>

{#if bots == null}
  <div class="card">
    {#if gameroom.busy}
      <p class="empty">Fetching the server's bots…</p>
    {:else}
      <p class="empty">The server's bots couldn't be listed.</p>
      <button onclick={() => gameroom.loadBots()}>Try again</button>
    {/if}
  </div>
{:else}
  <div class="panes">
    <section class="card" aria-labelledby="gr-bots-title">
      <div class="card-head">
        <h2 id="gr-bots-title">Server bots</h2>
        <span class="spacer"></span>
        <button class="subtle" onclick={() => gameroom.loadBots()} disabled={gameroom.busy}>Reload</button>
      </div>
      <div class="fields tools">
        <select bind:value={botSet} onchange={() => savePref('gameroom.botSet', botSet)} aria-label="Which bots">
          <option value="ladder" disabled={bots.ladder.length === 0}>Bot ladder</option>
          <option value="all">All bots</option>
        </select>
        <input bind:value={filter} placeholder="Filter by name" aria-label="Filter bots" autocomplete="off" />
        <select bind:value={order} aria-label="Order bots">
          <option value="ladder">Ladder order</option>
          <option value="rating">By rating</option>
          <option value="name">By name</option>
        </select>
      </div>
      {#if rows.length === 0}
        <p class="empty">{bots.bots.length === 0 ? 'arimaa.com lists no bots.' : 'No bot matches.'}</p>
      {:else}
        <ul class="list scroll" aria-label="Server bots">
          {#each rows as row ('me' in row ? '' : row.bot.name)}
            {#if 'me' in row}
              <li class="me">
                You{bots.ladderAbove > 0 ? ': beat the bots above to climb the ladder' : ', at the top of the ladder'}
              </li>
            {:else}
              {@const b = row.bot}
              <li class:selected={chosen?.bot.name === b.name}>
                <span class="row-text">
                  <span class="row-line">
                    <span class="name">{b.name}</span>
                    {#if b.rating != null}<span class="rating">{b.rating}</span>{/if}
                  </span>
                  {#if botRecord(b)}<span class="meta">{botRecord(b)}</span>{/if}
                </span>
                <button
                  onclick={() => gameroom.pickBot(b)}
                  disabled={gameroom.busy || chosen?.bot.name === b.name}
                  aria-label="Choose {b.name}">Choose</button
                >
              </li>
            {/if}
          {/each}
        </ul>
      {/if}
    </section>

    <section class="card detail" aria-label="Chosen bot">
      {#if !chosen}
        <p class="empty">Choose a bot to see its time control and start a game against it.</p>
      {:else}
        {@const info = chosen.info}
        <div class="card-head">
          <h2>{chosen.bot.name}</h2>
          {#if chosen.bot.rating != null}<span class="rating">{chosen.bot.rating}</span>{/if}
        </div>
        {#if info}
          <p class="meta">
            {[info.timeControl ?? '', info.rated == null ? '' : info.rated ? 'rated' : 'unrated']
              .filter((s) => s)
              .join(' · ')}
          </p>
          {#if info.about}<p class="about">{info.about}</p>{/if}
          {#if info.status}<p class="meta">{info.status}</p>{/if}
        {:else}
          <p class="meta">Reading its page…</p>
        {/if}
        <form onsubmit={start}>
          <div class="fields">
            <span id="gr-bot-side-label">You play</span>
            <SidePicker bind:value={userSide} name="gr-bot-side" labelledby="gr-bot-side-label" />
          </div>
          <label class="check"><input type="checkbox" bind:checked={join} /> Join the game as soon as it opens</label>
          <label class="check" title="Off: join in the gameroom's unrated mode, so a rated bot game isn't rated">
            <input type="checkbox" bind:checked={keepRated} disabled={!join || info?.rated === false} /> Keep it rated
          </label>
          <button class="primary" type="submit" disabled={gameroom.busy || info == null || !info.canStart}
            >Start {chosen.bot.name}</button
          >
        </form>
        <p class="hint">
          {#if info && !info.canStart}
            Players can't start this bot now.
          {:else}
            The bot opens a game in the gameroom;
            {join
              ? 'you join it as soon as it opens, on a new board.'
              : 'it waits in the open games for you or anyone.'}
          {/if}
        </p>
      {/if}
      {#if gameroom.botStatus}<p class="status" role="status">{gameroom.botStatus}</p>{/if}
    </section>
  </div>
{/if}

<style>
  .panes {
    display: grid;
    grid-template-columns: minmax(300px, 3fr) minmax(280px, 2fr);
    gap: 16px;
    align-items: start;
  }
  @media (max-width: 760px) {
    .panes {
      grid-template-columns: 1fr;
    }
  }
  .tools {
    margin-bottom: 8px;
  }
  .detail {
    position: sticky;
    top: 0;
    display: flex;
    flex-direction: column;
    gap: 8px;
  }
  .detail .card-head {
    margin: 0;
  }
  .detail p {
    margin: 0;
  }
  .about {
    font-size: 13px;
  }
  form {
    display: flex;
    flex-direction: column;
    gap: 8px;
    margin-top: 6px;
    padding-top: 10px;
    border-top: 1px solid var(--border);
  }
  form button {
    align-self: flex-start;
  }
  .status {
    font-size: 13px;
    padding: 6px 8px;
    border-radius: 6px;
    background: var(--hover);
  }
  .list li.me {
    font-size: 12px;
    color: var(--muted);
    font-weight: 600;
    background: var(--bg);
  }
  .detail .meta {
    white-space: normal;
  }
</style>
