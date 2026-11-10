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
