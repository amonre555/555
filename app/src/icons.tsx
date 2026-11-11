import type { JSX } from "solid-js";

const Svg = (props: { children: JSX.Element; size?: number }) => (
  <svg
    width={props.size ?? 20}
    height={props.size ?? 20}
    viewBox="0 0 24 24"
    fill="none"
    stroke="currentColor"
    stroke-width="2"
    stroke-linecap="round"
    stroke-linejoin="round"
    aria-hidden="true"
  >
    {props.children}
  </svg>
);

export const IconPlay = (p: { size?: number }) => (
  <Svg size={p.size}>
    <path d="M7 4.5v15l12-7.5z" fill="currentColor" stroke="none" />
  </Svg>
);
export const IconPause = (p: { size?: number }) => (
  <Svg size={p.size}>
    <rect x="6" y="4.5" width="4" height="15" rx="1" fill="currentColor" stroke="none" />
    <rect x="14" y="4.5" width="4" height="15" rx="1" fill="currentColor" stroke="none" />
  </Svg>
);
export const IconNext = () => (
  <Svg>
    <path d="M5 5l10 7-10 7z" fill="currentColor" stroke="none" />
    <path d="M19 5v14" />
  </Svg>
);
export const IconPrev = () => (
  <Svg>
    <path d="M19 5L9 12l10 7z" fill="currentColor" stroke="none" />
    <path d="M5 5v14" />
  </Svg>
);
export const IconSearch = () => (
  <Svg size={18}>
    <circle cx="11" cy="11" r="7" />
    <path d="M20 20l-3.5-3.5" />
  </Svg>
);
export const IconCompass = () => (
  <Svg>
    <circle cx="12" cy="12" r="9" />
    <path d="M15.5 8.5l-2 5-5 2 2-5z" />
  </Svg>
);
export const IconLibrary = () => (
  <Svg>
    <path d="M4 4v16M9 4v16" />
    <path d="M14 4.5l5 15" />
  </Svg>
);
export const IconFolderPlus = () => (
  <Svg size={18}>
    <path d="M3 7a2 2 0 0 1 2-2h4l2 2h8a2 2 0 0 1 2 2v8a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2z" />
    <path d="M12 10v6M9 13h6" />
  </Svg>
);
export const IconRefresh = () => (
  <Svg size={18}>
    <path d="M20 11a8 8 0 1 0-2.3 5.7" />
    <path d="M20 4v7h-7" />
  </Svg>
);
export const IconClose = () => (
  <Svg size={14}>
    <path d="M6 6l12 12M18 6L6 18" />
  </Svg>
);
export const IconVolume = (p: { level: number }) => (
  <Svg size={18}>
    <path d="M4 9v6h4l5 4V5L8 9z" fill="currentColor" stroke="none" />
    {p.level > 0 && <path d="M16 9.5a3.5 3.5 0 0 1 0 5" />}
    {p.level > 0.5 && <path d="M18.5 7a7 7 0 0 1 0 10" />}
  </Svg>
);
export const IconNote = () => (
  <Svg size={22}>
    <path d="M9 18V5l11-2v13" />
    <circle cx="6" cy="18" r="3" />
    <circle cx="17" cy="16" r="3" />
  </Svg>
);
export const IconLyrics = () => (
  <Svg size={18}>
    <path d="M4 6h10M4 11h16M4 16h7" />
    <circle cx="17.5" cy="16.5" r="2.5" />
    <path d="M20 16.5V8.5l2-1" />
  </Svg>
);
export const IconChevronDown = () => (
  <Svg size={22}>
    <path d="M6 9l6 6 6-6" />
  </Svg>
);
export const IconBack = () => (
  <Svg size={20}>
    <path d="M15 18l-6-6 6-6" />
  </Svg>
);
export const IconUser = () => (
  <Svg size={40}>
    <circle cx="12" cy="8" r="4" />
    <path d="M4 21c0-4 4-6 8-6s8 2 8 6" />
  </Svg>
);
export const IconHeart = (p: { filled?: boolean; size?: number }) => (
  <Svg size={p.size ?? 18}>
    <path
      d="M12 20.5s-7.5-4.6-7.5-10.2A4.3 4.3 0 0 1 12 7.6a4.3 4.3 0 0 1 7.5 2.7c0 5.6-7.5 10.2-7.5 10.2z"
      fill={p.filled ? "currentColor" : "none"}
    />
  </Svg>
);
export const IconMore = () => (
  <Svg size={18}>
    <circle cx="5.5" cy="12" r="1.4" fill="currentColor" stroke="none" />
    <circle cx="12" cy="12" r="1.4" fill="currentColor" stroke="none" />
    <circle cx="18.5" cy="12" r="1.4" fill="currentColor" stroke="none" />
  </Svg>
);
export const IconGrip = () => (
  <Svg size={18}>
    <path d="M5 9h14M5 15h14" />
  </Svg>
);
export const IconPlus = (p: { size?: number }) => (
  <Svg size={p.size ?? 18}>
    <path d="M12 5v14M5 12h14" />
  </Svg>
);
export const IconPlaylist = (p: { size?: number }) => (
  <Svg size={p.size ?? 20}>
    <path d="M4 6h12M4 11h12M4 16h7" />
    <circle cx="17" cy="17" r="2.5" />
    <path d="M19.5 17V9l2-.8" />
  </Svg>
);
export const IconTrash = () => (
  <Svg size={18}>
    <path d="M4 7h16M10 11v6M14 11v6M6 7l1 12a2 2 0 0 0 2 2h6a2 2 0 0 0 2-2l1-12M9 7V4h6v3" />
  </Svg>
);
export const IconEdit = () => (
  <Svg size={18}>
    <path d="M4 20h4L19 9l-4-4L4 16z" />
    <path d="M13.5 6.5l4 4" />
  </Svg>
);
export const IconImage = () => (
  <Svg size={18}>
    <rect x="3.5" y="4.5" width="17" height="15" rx="2.5" />
    <circle cx="9" cy="10" r="1.6" />
    <path d="M20.5 16l-5-5-9 8.5" />
  </Svg>
);
export const IconMinus = (p: { size?: number }) => (
  <Svg size={p.size ?? 18}>
    <path d="M5 12h14" />
  </Svg>
);
export const IconDownload = () => (
  <Svg size={16}>
    <path d="M12 4v11M7 10l5 5 5-5M5 20h14" />
  </Svg>
);
export const IconShuffle = () => (
  <Svg size={18}>
    <path d="M3 7h3.5c2 0 3.2 1 4.3 2.7l2.4 4.6c1.1 1.7 2.3 2.7 4.3 2.7H21" />
    <path d="M3 17h3.5c1.3 0 2.3-.4 3.1-1.2M14.4 8.2c.8-.8 1.8-1.2 3.1-1.2H21" />
    <path d="M18 4l3 3-3 3M18 14l3 3-3 3" />
  </Svg>
);
export const IconRepeat = (p: { one?: boolean }) => (
  <Svg size={18}>
    <path d="M4 11V9a3 3 0 0 1 3-3h13M17 3l3 3-3 3" />
    <path d="M20 13v2a3 3 0 0 1-3 3H4M7 21l-3-3 3-3" />
    {p.one && <path d="M11.5 10.2 12.6 9.5v5" stroke-width="1.8" />}
  </Svg>
);
export const IconSettings = () => (
  <Svg>
    <circle cx="12" cy="12" r="3" />
    <path d="M19.4 15a1.7 1.7 0 0 0 .3 1.8l.1.1a2 2 0 1 1-2.8 2.8l-.1-.1a1.7 1.7 0 0 0-1.8-.3 1.7 1.7 0 0 0-1 1.5V21a2 2 0 1 1-4 0v-.1a1.7 1.7 0 0 0-1.1-1.5 1.7 1.7 0 0 0-1.8.3l-.1.1a2 2 0 1 1-2.8-2.8l.1-.1a1.7 1.7 0 0 0 .3-1.8 1.7 1.7 0 0 0-1.5-1H3a2 2 0 1 1 0-4h.1a1.7 1.7 0 0 0 1.5-1.1 1.7 1.7 0 0 0-.3-1.8l-.1-.1a2 2 0 1 1 2.8-2.8l.1.1a1.7 1.7 0 0 0 1.8.3H9a1.7 1.7 0 0 0 1-1.5V3a2 2 0 1 1 4 0v.1a1.7 1.7 0 0 0 1 1.5 1.7 1.7 0 0 0 1.8-.3l.1-.1a2 2 0 1 1 2.8 2.8l-.1.1a1.7 1.7 0 0 0-.3 1.8V9a1.7 1.7 0 0 0 1.5 1H21a2 2 0 1 1 0 4h-.1a1.7 1.7 0 0 0-1.5 1z" />
  </Svg>
);
export const IconLink = () => (
  <Svg size={18}>
    <path d="M10 14a4 4 0 0 0 5.7 0l3-3a4 4 0 0 0-5.7-5.7l-1 1" />
    <path d="M14 10a4 4 0 0 0-5.7 0l-3 3a4 4 0 0 0 5.7 5.7l1-1" />
  </Svg>
);
export const IconClock = (p: { size?: number }) => (
  <Svg size={p.size}>
    <circle cx="12" cy="12" r="9" />
    <path d="M12 7v5l3 2" />
  </Svg>
);
export const IconGlobe = (p: { size?: number }) => (
  <Svg size={p.size}>
    <circle cx="12" cy="12" r="9" />
    <path d="M3 12h18M12 3c2.5 2.7 3.8 5.7 3.8 9s-1.3 6.3-3.8 9c-2.5-2.7-3.8-5.7-3.8-9S9.5 5.7 12 3z" />
  </Svg>
);
export const IconChart = (p: { size?: number }) => (
  <Svg size={p.size}>
    <path d="M5 20V11M10 20V5M15 20v-7M20 20V9" />
  </Svg>
);
/** Ondes concentriques : le récap de l'année. */
export const IconWaves = (p: { size?: number }) => (
  <Svg size={p.size}>
    <circle cx="12" cy="12" r="2" fill="currentColor" stroke="none" />
    <path d="M7.8 7.8a6 6 0 0 0 0 8.4M16.2 7.8a6 6 0 0 1 0 8.4M4.9 4.9a10 10 0 0 0 0 14.2M19.1 4.9a10 10 0 0 1 0 14.2" />
  </Svg>
);
