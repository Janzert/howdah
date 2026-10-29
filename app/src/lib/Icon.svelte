<script lang="ts" module>
  /** Line icons, drawn on a 24-unit grid with the current text color. */
  const PATHS = {
    home: 'M3 11.5 12 4l9 7.5M5.5 9.5V20h4.5v-6h4v6h4.5V9.5',
    globe: 'M3 12h18M12 3c3.2 3.2 3.2 14.8 0 18M12 3c-3.2 3.2-3.2 14.8 0 18M12 3a9 9 0 1 0 0 18 9 9 0 0 0 0-18',
    flask: 'M9 3h6M10 3v6.5L4.8 18.2A1.9 1.9 0 0 0 6.4 21h11.2a1.9 1.9 0 0 0 1.6-2.8L14 9.5V3M7.5 15h9',
    cpu: 'M7 6h10a1 1 0 0 1 1 1v10a1 1 0 0 1-1 1H7a1 1 0 0 1-1-1V7a1 1 0 0 1 1-1ZM10 10h4v4h-4zM9.5 2.5V6M14.5 2.5V6M9.5 18v3.5M14.5 18v3.5M2.5 9.5H6M2.5 14.5H6M18 9.5h3.5M18 14.5h3.5',
    gear: 'M12 9a3 3 0 1 0 0 6 3 3 0 0 0 0-6M12 2.5v3M12 18.5v3M2.5 12h3M18.5 12h3M5.3 5.3l2.1 2.1M16.6 16.6l2.1 2.1M5.3 18.7l2.1-2.1M16.6 7.4l2.1-2.1',
    help: 'M12 3a9 9 0 1 0 0 18 9 9 0 0 0 0-18M9.6 9.3a2.5 2.5 0 1 1 3.6 2.2c-.7.4-1.2 1-1.2 1.8v.7M12 16.8v.4',
    collapse: 'M4 4h16v16H4zM9.5 4v16M16 9.5 13.5 12l2.5 2.5',
    expand: 'M4 4h16v16H4zM9.5 4v16M13.5 9.5 16 12l-2.5 2.5',
    plus: 'M12 5v14M5 12h14',
    board: 'M4 4h16v16H4zM4 9.3h16M4 14.7h16M9.3 4v16M14.7 4v16',
    record: 'M6 3h8.5L19 7.5V21H6zM14 3v5h5M9 12.5h7M9 16h7',
    edit: 'M4 20l1-4.5L15.5 5a2.1 2.1 0 0 1 3 3L8 18.5zM13.5 7l3 3',
    bulb: 'M9.5 18h5M10.5 21h3M12 3a6 6 0 0 0-3.9 10.6c.7.6 1.1 1.4 1.1 2.4h5.6c0-1 .4-1.8 1.1-2.4A6 6 0 0 0 12 3',
    refresh: 'M19.5 12a7.5 7.5 0 1 1-2.2-5.3M19.5 4v4.5H15',
    chat: 'M4 5h16v11H9l-5 4z',
    calendar: 'M4 6h16v14H4zM4 10.5h16M8.5 3.5V8M15.5 3.5V8M8 14h2M12 14h2M16 14h0M8 17h2M12 17h2',
    bot: 'M7 8h10a2 2 0 0 1 2 2v7a2 2 0 0 1-2 2H7a2 2 0 0 1-2-2v-7a2 2 0 0 1 2-2ZM12 4v4M9.5 13v1M14.5 13v1M2.5 12.5v3M21.5 12.5v3',
    window: 'M3.5 5h17v14h-17zM3.5 9h17',
    close: 'M6 6l12 12M18 6 6 18',
    eye: 'M2.5 12s3.5-6.5 9.5-6.5 9.5 6.5 9.5 6.5-3.5 6.5-9.5 6.5S2.5 12 2.5 12ZM12 9.2a2.8 2.8 0 1 0 0 5.6 2.8 2.8 0 0 0 0-5.6',
    users:
      'M9 11a3.5 3.5 0 1 0 0-7 3.5 3.5 0 0 0 0 7M2.5 20c0-3.6 2.9-6 6.5-6s6.5 2.4 6.5 6M16 4.3a3.5 3.5 0 0 1 0 6.4M18.5 14.4c1.8.8 3 2.7 3 5.6',
    offline: 'M2.5 9a14 14 0 0 1 19 0M5.5 12.5a9.5 9.5 0 0 1 13 0M8.8 16a5 5 0 0 1 6.4 0M12 19.5v.1M3.5 3.5l17 17',
    first: 'M6 5.5v13M18 6.5 10 12l8 5.5z',
    prev: 'M16 6.5 8 12l8 5.5z',
    next: 'M8 6.5 16 12l-8 5.5z',
    fastForward: 'M4 6.5 11 12l-7 5.5zM13 6.5 20 12l-7 5.5z',
    pause: 'M8.5 5.5v13M15.5 5.5v13',
    last: 'M18 5.5v13M6 6.5 14 12l-8 5.5z',
    popout: 'M14 4h6v6M20 4l-8.5 8.5M18 13.5V19a1 1 0 0 1-1 1H5a1 1 0 0 1-1-1V7a1 1 0 0 1 1-1h5.5',
    sound: 'M4 9.5h3.5L12 5.5v13l-4.5-4H4zM15.5 9.5a3.5 3.5 0 0 1 0 5M18 7a7 7 0 0 1 0 10',
    soundLow: 'M4 9.5h3.5L12 5.5v13l-4.5-4H4zM15.5 9.5a3.5 3.5 0 0 1 0 5',
    soundOff: 'M4 9.5h3.5L12 5.5v13l-4.5-4H4zM15.5 9.5l5 5M20.5 9.5l-5 5',
  } as const;

  export type IconName = keyof typeof PATHS;
</script>

<script lang="ts">
  let { name, size = 18 }: { name: IconName; size?: number } = $props();
</script>

<svg
  width={size}
  height={size}
  viewBox="0 0 24 24"
  fill="none"
  stroke="currentColor"
  stroke-width="1.7"
  stroke-linecap="round"
  stroke-linejoin="round"
  aria-hidden="true"
>
  <path d={PATHS[name]} />
</svg>

<style>
  svg {
    flex: none;
    display: block;
  }
</style>
