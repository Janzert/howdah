// The arimaa.com lobby's state, shared by the lobby window's arimaa.com
// screen and its tabs, so switching tabs or screens keeps what was loaded
// (the bots, a player's games, a search). One instance, `gameroom`, in the
// lobby window. Playing, watching or opening a game opens a game window
// for it, through the opener the lobby sets (`setOpener`).
import { type Api, api, errorMessage } from '../api';
import type { AnnouncementView } from '../bindings/AnnouncementView';
import type { BotInfoView } from '../bindings/BotInfoView';
import type { Color } from '../bindings/Color';
import type { GameroomGames } from '../bindings/GameroomGames';
import type { InvitationView } from '../bindings/InvitationView';
import type { LiveGameView } from '../bindings/LiveGameView';
import type { PastGameView } from '../bindings/PastGameView';
import type { PlayerMatchView } from '../bindings/PlayerMatchView';
import type { PostalGameView } from '../bindings/PostalGameView';
import type { ServerBotView } from '../bindings/ServerBotView';
import type { ServerBotsView } from '../bindings/ServerBotsView';
import { on } from '../events';
import { announcementKey, mySide, myTurn, splitAnnouncements } from '../gameroom';

export function pref(key: string): string {
  try {
    return localStorage.getItem(key) ?? '';
  } catch {
    return '';
  }
}

export function savePref(key: string, value: string) {
  try {
    localStorage.setItem(key, value);
  } catch {
    /* not persisted */
  }
}

/** The arimaa.com panels opened from the overview, each listed under
 * arimaa.com in the lobby's sidebar once opened. */
export type GameroomTab = 'bots' | 'watch' | 'finished' | 'players';

export const GAMEROOM_PANELS: { id: GameroomTab; label: string; icon: 'bot' | 'eye' | 'record' | 'users' }[] = [
  { id: 'bots', label: 'Bots', icon: 'bot' },
  { id: 'watch', label: 'Watch', icon: 'eye' },
  { id: 'finished', label: 'Finished games', icon: 'record' },
  { id: 'players', label: 'Players', icon: 'users' },
];

/** Opens a game window whose session `start` makes a gameroom game
 * (opening, joining or creating one), or brings forward the window that
 * has `game` already. Resolves to an error message, or null. */
export type Opener = (start: (a: Api) => Promise<void>, game?: { gid: string; play: boolean }) => Promise<string | null>;

/** What the New game dialog asks for. */
export interface NewGameSpec {
  side: Color | 'random';
  timeControl: string;
  rated: boolean;
  /** A player to invite; empty opens the game to anyone. */
  opponent: string;
  message: string;
}

/** How often the lists are refreshed while the arimaa.com screen is shown,
 * as the browser lobby does. */
const REFRESH_MS = 20_000;

const pickSide = (side: Color | 'random'): Color =>
  side === 'random' ? (Math.random() < 0.5 ? 'gold' : 'silver') : side;

class GameroomStore {
  /** Who is logged in; undefined until the backend says. */
  user = $state<string | null | undefined>(undefined);
  /** The username whose password the backend has saved. */
  savedUser = $state<string | null>(null);
  games = $state<GameroomGames | null>(null);
  /** When the lists last arrived (performance.now()). */
  updatedAt = $state<number | null>(null);
  busy = $state(false);
  error = $state<string | null>(null);

  /** The server bots, once asked for. */
  bots = $state<ServerBotsView | null>(null);
  /** The bot picked to play, with what its page says once fetched. */
  bot = $state<{ bot: ServerBotView; info: BotInfoView | null } | null>(null);
  /** What's happening with a started bot, or what the server said. */
  botStatus = $state<string | null>(null);

  /** The postal games being played, once asked for. */
  postal = $state<PostalGameView[] | null>(null);

  /** The text typed into the player search. */
  playerQuery = $state('');
  /** The players the last search found. */
  players = $state<PlayerMatchView[] | null>(null);
  /** The player whose games are shown, with the pages loaded so far. */
  picked = $state<{ player: PlayerMatchView; games: PastGameView[]; next: number | null } | null>(null);

  /** The New game dialog, open with these starting values. */
  newGame = $state<Partial<NewGameSpec> | null>(null);

  /** Whether the overview shows the older (and hidden) announcements too. */
  showOlderAnnouncements = $state(false);

  /** Announcements the user hid, by `announcementKey`. */
  hiddenAnnouncements = $state(new Set(pref('gameroom.hiddenAnnouncements').split('\n').filter((k) => k)));

  #opener: Opener = async () => 'No lobby';
  #navigate: (panel: GameroomTab) => void = () => {};
  #onGames: ((g: GameroomGames) => void) | null = null;
  #started = false;

  /** Each player's `idle` when first seen in the lists for `#seenFor`.
   * Invite waits until it moves: a client that asks the lobby for
   * anything gets its account's invitation setting applied (a new login
   * shows invitations on until then), while a bot on the bot API, which
   * never sees invitations, never moves it. */
  #firstIdle = new Map<string, number | null>();
  #seenFor: string | null = null;

  /** Who is logged in, by name. */
  online = $derived(
    [...(this.games?.online ?? [])].sort((a, b) =>
      a.username.localeCompare(b.username, undefined, { sensitivity: 'base' }),
    ),
  );

  /** The players to offer Invite for: those who take invitations, aren't
   * busy, and have used the lobby since they were first seen. A bot may
   * be busy: one that passes is run by an invitation controller, which
   * can play several games at once and turns its own flag off when it's
   * full. */
  invitable = $derived(
    new Set(
      this.online
        .filter((p) => p.inviteMe && (!p.busy || p.bot) && p.username !== this.user)
        .filter((p) => p.idle != null && p.idle > (this.#firstIdle.get(p.username) ?? Infinity))
        .map((p) => p.username),
    ),
  );

  incoming = $derived(this.games?.invitations.filter((i) => i.incoming) ?? []);
  outgoing = $derived(this.games?.invitations.filter((i) => !i.incoming) ?? []);

  /** The user's games, those waiting on their move first. */
  myGames = $derived(
    this.games ? [...this.games.mine].sort((a, b) => Number(this.myTurn(b)) - Number(this.myTurn(a))) : [],
  );

  announcements = $derived(splitAnnouncements(this.games?.announcements ?? [], this.hiddenAnnouncements));

  mySide = (g: LiveGameView) => mySide(g, this.user);
  myTurn = (g: LiveGameView) => myTurn(g, this.user);

  setOpener(opener: Opener) {
    this.#opener = opener;
  }

  /** How `show` opens a panel (the lobby's sidebar). */
  setNavigator(navigate: (panel: GameroomTab) => void) {
    this.#navigate = navigate;
  }

  /** Opens one of the arimaa.com panels (and lists it in the sidebar). */
  show(panel: GameroomTab) {
    this.#navigate(panel);
  }

  /** Asks the backend who is logged in and follows its lobby poll; once.
   * `onGames` gets the lists whenever they arrive here. */
  start(onGames: (g: GameroomGames) => void): () => void {
    this.#onGames = onGames;
    if (this.#started) return () => {};
    this.#started = true;
    // The backend's poll, every 50 s while logged in.
    const unlisten = on('gameroom://lobby', (g) => {
      if (this.user && g.user === this.user) this.setGames(g);
    });
    api.gameroomStatus().then((s) => {
      this.savedUser = s.savedUsername;
      this.setUser(s.username);
      if (this.user) this.refresh();
    });
    return () => {
      this.#started = false;
      void unlisten.then((f) => f());
    };
  }

  /** Refreshes the lists every `REFRESH_MS` while the window is shown;
   * returns the function that stops it. */
  autoRefresh(): () => void {
    const timer = setInterval(() => {
      if (this.user && !this.busy && !document.hidden) this.refresh(true);
    }, REFRESH_MS);
    return () => clearInterval(timer);
  }

  setGames(g: GameroomGames) {
    if (!g.user || g.user !== this.user) {
      this.#seenFor = null;
    } else {
      // Newcomers are recorded before `invitable` sees the new list, so
      // they aren't offered until their idle moves.
      const known = this.#seenFor === g.user ? this.#firstIdle : new Map<string, number | null>();
      this.#seenFor = g.user;
      this.#firstIdle = new Map(
        g.online.map((p) => [p.username, known.has(p.username) ? known.get(p.username)! : p.idle]),
      );
    }
    this.games = g;
    this.updatedAt = performance.now();
    this.#onGames?.(g);
  }

  setUser(user: string | null) {
    if (user !== this.user) {
      // The bot list holds a user's record and place on the ladder.
      this.bots = null;
      this.bot = null;
      this.botStatus = null;
    }
    this.user = user;
    if (!user) this.games = null;
  }

  /** Runs a request, showing its error. A `quiet` one (the timed refresh)
   * leaves the buttons alone. After an error the login is checked again,
   * since an expired one logs out. */
  async attempt(f: () => Promise<void>, quiet = false) {
    if (!quiet) {
      this.busy = true;
      this.error = null;
    }
    try {
      await f();
      if (quiet) this.error = null;
    } catch (e) {
      this.error = errorMessage(e);
      this.setUser((await api.gameroomStatus()).username);
    } finally {
      if (!quiet) this.busy = false;
    }
  }

  refresh(quiet = false) {
    return this.attempt(async () => this.setGames(await api.gameroomGames()), quiet);
  }

  login(username: string, password: string, remember: boolean) {
    return this.attempt(async () => {
      const s = await api.gameroomLogin(username, password, remember);
      this.savedUser = s.savedUsername;
      this.setUser(s.username);
      savePref('gameroom.username', username.trim());
      // The login fetched the lists (they say who the user is).
      this.setGames((await api.gameroomLastGames()) ?? (await api.gameroomGames()));
    });
  }

  logout() {
    return this.attempt(async () => {
      await api.gameroomLogout();
      this.setUser(null);
    });
  }

  async enter(start: (a: Api) => Promise<void>, game?: { gid: string; play: boolean }) {
    this.busy = true;
    this.error = await this.#opener(start, game);
    this.busy = false;
  }

  /** Watches a live game, or loads a finished one, by id. */
  open(gid: string) {
    const id = gid.trim();
    return this.enter((a) => a.openGameroomGame(id), { gid: id, play: false });
  }

  play(gid: string, side: Color, unrated = false) {
    return this.enter((a) => a.playGameroomGame(gid, side, unrated), { gid, play: true });
  }

  accept(i: InvitationView) {
    return this.enter((a) => a.acceptGameroomInvitation(i.otherId, i.created));
  }

  decline(i: InvitationView) {
    return this.attempt(async () => {
      await api.declineGameroomInvitation(i.otherId, i.created, '');
      this.setGames(await api.gameroomGames());
    });
  }

  cancelInvitation(i: InvitationView) {
    return this.attempt(async () => {
      await api.cancelGameroomInvitation(i.otherId, i.created);
      this.setGames(await api.gameroomGames());
    });
  }

  cancelGame(gid: string) {
    return this.attempt(async () => {
      await api.cancelGameroomGame(gid);
      this.setGames(await api.gameroomGames());
    });
  }

  /** Creates a game open to anyone (it opens in a window), or invites a
   * player (it waits for the answer; accepted, the game opens). Resolves
   * to whether it went through. */
  async createGame(spec: NewGameSpec): Promise<boolean> {
    const side = pickSide(spec.side);
    const tc = spec.timeControl.trim();
    const opponent = spec.opponent.trim();
    if (opponent) {
      await this.attempt(async () => {
        await api.inviteGameroomPlayer(opponent, side, tc, spec.rated, spec.message);
        this.setGames(await api.gameroomGames());
      });
    } else {
      await this.enter((a) => a.createGameroomGame(side, tc, spec.rated));
    }
    return this.error == null;
  }

  loadBots() {
    return this.attempt(async () => {
      this.bots = await api.gameroomServerBots();
    });
  }

  pickBot(b: ServerBotView) {
    this.bot = { bot: b, info: null };
    this.botStatus = null;
    return this.attempt(async () => {
      const info = await api.gameroomBotInfo(b.page);
      if (this.bot?.bot.name === b.name) this.bot = { bot: b, info };
    });
  }

  /** Starts the picked bot with the user at `userSide`; joining, plays the
   * game it opens (`unrated`: in the gameroom's unrated mode). */
  startBot(userSide: Color | 'random', join: boolean, unrated: boolean) {
    const picked = this.bot;
    if (!picked) return;
    const side = pickSide(userSide);
    const botSide: Color = side === 'gold' ? 'silver' : 'gold';
    const { name, page } = picked.bot;
    this.botStatus = `Starting ${name}; waiting for its game to open…`;
    return this.attempt(async () => {
      let started;
      try {
        started = await api.startGameroomBot(page, name, botSide);
      } catch (e) {
        this.botStatus = null;
        throw e;
      }
      const said = started.message ? ` arimaa.com said: “${started.message}”` : '';
      this.setGames(await api.gameroomGames());
      const gid = started.gid;
      if (gid == null) {
        this.botStatus = `${name} didn't open a game in time.${said}`;
        return;
      }
      this.botStatus = join
        ? `${name} opened game ${gid}.${said}`
        : `${name} opened game ${gid}; it's in the open games.${said}`;
      if (join) await this.play(gid, side, unrated && started.rated);
    });
  }

  loadPostal() {
    return this.attempt(async () => {
      this.postal = await api.gameroomPostalGames();
    });
  }

  searchPlayers(text: string) {
    const query = text.trim();
    return this.attempt(async () => {
      const found = await api.searchGameroomPlayers(query);
      this.players = found;
      this.picked = null;
      // An exact username goes straight to that player's games.
      const exact = found.filter((p) => p.username.toLowerCase() === query.toLowerCase());
      if (exact.length === 1) await this.#pick(exact[0]);
    });
  }

  /** Shows a player's games on the Finished games panel. */
  showPlayer(username: string) {
    this.playerQuery = username;
    this.show('finished');
    return this.searchPlayers(username);
  }

  pick(player: PlayerMatchView) {
    return this.attempt(() => this.#pick(player));
  }

  async #pick(player: PlayerMatchView) {
    const page = await api.gameroomPlayerGames(player.id, 0);
    this.picked = { player, games: page.games, next: page.next };
  }

  olderGames() {
    const p = this.picked;
    if (!p || p.next == null) return;
    const next = p.next;
    return this.attempt(async () => {
      const page = await api.gameroomPlayerGames(p.player.id, next);
      this.picked = { player: p.player, games: [...p.games, ...page.games], next: page.next };
    });
  }

  hideAnnouncement(a: AnnouncementView) {
    // Only the keys of announcements still listed are kept.
    const listed = new Set((this.games?.announcements ?? []).map(announcementKey));
    this.hiddenAnnouncements = new Set(
      [...this.hiddenAnnouncements, announcementKey(a)].filter((k) => listed.has(k)),
    );
    savePref('gameroom.hiddenAnnouncements', [...this.hiddenAnnouncements].join('\n'));
  }

  async openLink(e: MouseEvent, url: string) {
    e.preventDefault();
    try {
      await api.openUrl(url);
    } catch (err) {
      this.error = errorMessage(err);
    }
  }
}

export const gameroom = new GameroomStore();
