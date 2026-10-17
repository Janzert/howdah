<script lang="ts">
  // The gameroom login, shown on the arimaa.com screen until the user logs
  // in. A saved login fills the username, and the password field says
  // "Saved password".
  import { untrack } from 'svelte';
  import { gameroom, pref } from './store.svelte';

  let username = $state(untrack(() => gameroom.savedUser) ?? pref('gameroom.username'));
  let password = $state('');
  let remember = $state(untrack(() => gameroom.savedUser != null));
  const usesSaved = $derived(gameroom.savedUser != null && username.trim() === gameroom.savedUser && password === '');

  function login(e: SubmitEvent) {
    e.preventDefault();
    void gameroom.login(username, password, remember).then(() => (password = ''));
  }
</script>

<form class="card login" onsubmit={login} aria-labelledby="gr-login-title">
  <h2 id="gr-login-title">Log in to the gameroom</h2>
  <p class="hint">
    Playing, watching, inviting and opening games on arimaa.com need a gameroom login (a human account).
  </p>
  <label for="gr-user">Username or email</label>
  <!-- svelte-ignore a11y_autofocus -->
  <input id="gr-user" bind:value={username} autocomplete="username" autofocus={!username} />
  <label for="gr-password">Password</label>
  <!-- svelte-ignore a11y_autofocus -->
  <input
    id="gr-password"
    type="password"
    bind:value={password}
    autocomplete="current-password"
    autofocus={!!username}
    placeholder={gameroom.savedUser != null && username.trim() === gameroom.savedUser ? 'Saved password' : ''}
  />
  <label class="check">
    <input type="checkbox" bind:checked={remember} />
    Remember password
  </label>
  <p class="hint small">
    The password goes over plain HTTP, as arimaa.com requires.
    {#if remember}
      It's saved on this computer obfuscated, not encrypted: anyone who can read your files could recover it.
    {:else}
      It isn't saved{gameroom.savedUser ? ', and logging in forgets the saved one' : ''}.
    {/if}
  </p>
  {#if gameroom.error}<p class="error" role="alert">{gameroom.error}</p>{/if}
  <button class="primary" type="submit" disabled={gameroom.busy || !username.trim() || (!password && !usesSaved)}>
    {gameroom.busy ? 'Logging in…' : 'Log in'}
  </button>
</form>

<style>
  .login {
    display: flex;
    flex-direction: column;
    gap: 6px;
    width: min(380px, 100%);
    margin: 40px auto 0;
    padding: 22px 24px;
  }
  h2 {
    margin: 0;
    font-size: 17px;
  }
  .hint {
    margin-bottom: 8px;
  }
  .small {
    margin: 4px 0 6px;
  }
  label:not(.check) {
    font-size: 13px;
    margin-top: 4px;
  }
  .check {
    margin-top: 6px;
  }
  button {
    align-self: flex-end;
    padding: 6px 18px;
  }
</style>
