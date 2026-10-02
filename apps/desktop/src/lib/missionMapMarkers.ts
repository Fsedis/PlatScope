import type { MissionObject, MissionObjectKind } from "./missionResearch";
import { missionMarkerRadius } from "./missionMapPreferences";

export interface MissionMarkerGlyph {
  outline: string;
  detail?: string;
}

// Одна геометрия для значков на карте, в фильтрах и списке объектов.
export const MISSION_MARKER_GLYPHS: Record<MissionObjectKind, MissionMarkerGlyph> = {
  cache: {
    outline: "M4 7 12 3l8 4v10l-8 4-8-4Z",
    detail: "m4 7 8 4 8-4M12 11v10M8 5l8 4",
  },
  feather: {
    outline: "M6 18C4 12 8 4 19 3c1 10-4 15-10 15Z",
    detail: "M4 21 16 8M9 16l-1-5m5 1 4-1",
  },
  extraction: {
    outline: "M10 4H4v16h6",
    detail: "M8 12h13m-5-5 5 5-5 5",
  },
  dragon_door: {
    outline: "M5 21V10a7 7 0 0 1 14 0v11h-4V10a3 3 0 0 0-6 0v11Z",
    detail: "M3 21h18M5 13h4m6 0h4M12 3V1",
  },
  pickup: {
    outline: "m12 3 8 9-8 9-8-9Z",
    detail: "m12 7 4 5-4 5-4-5Z",
  },
  decree_fragment: {
    outline: "m13 2 7 6-5 13-9-3-2-8Z",
    detail: "m13 2-3 9 5 10M4 10l6 1 10-3",
  },
  avatar: {
    outline: "m12 2 9 19-9-5-9 5Z",
    detail: "M12 7v9",
  },
  npc: {
    outline: "M15 7a3 3 0 1 1-6 0 3 3 0 0 1 6 0ZM6 21v-4a6 6 0 0 1 12 0v4Z",
  },
  hostage: {
    outline: "M15 6a3 3 0 1 1-6 0 3 3 0 0 1 6 0ZM6 20v-4a6 6 0 0 1 12 0v4Z",
    detail: "M9 16h6m-3-3v6",
  },
  spawnpoint: {
    outline: "M19 12a7 7 0 1 1-14 0 7 7 0 0 1 14 0Z",
    detail: "M12 2v4m0 12v4M2 12h4m12 0h4m-8-8 4 4-4 4-4-4Z",
  },
  panel: {
    outline: "M4 5h16v14H4Z",
    detail: "M7 8h7m-7 4h10M8 16h1m3 0h1m3 0h1",
  },
  locker: {
    outline: "M6 3h12v18H6Z",
    detail: "M9 6h6M9 9h6M14 13v3M12 3v18",
  },
  terminal: {
    outline: "M3 4h18v13H3Z",
    detail: "m7 8 3 3-3 3m6 0h4M12 17v4m-5 0h10",
  },
  lootspot: {
    outline: "M20 10a8 8 0 1 0-16 0c0 5 8 11 8 11s8-6 8-11Z",
    detail: "m12 6 4 4-4 4-4-4Z",
  },
  decoration: {
    outline: "m12 4 8 8-8 8-8-8Z",
    detail: "M12 9v6M9 12h6",
  },
};

export interface MissionMapMarkerOptions {
  size: number;
  color: string;
  selected: boolean;
  target: boolean;
  local: boolean;
  // Радианы относительно поворота карты; ноль направлен вверх.
  heading: number;
}

const TARGET_COLOR = "#edc581";
const markerPaths = new Map<MissionObjectKind, { outline: Path2D; detail: Path2D | null }>();

function pathsFor(kind: MissionObjectKind): { outline: Path2D; detail: Path2D | null } {
  let paths = markerPaths.get(kind);
  if (!paths) {
    const glyph = MISSION_MARKER_GLYPHS[kind];
    paths = { outline: new Path2D(glyph.outline), detail: glyph.detail ? new Path2D(glyph.detail) : null };
    markerPaths.set(kind, paths);
  }
  return paths;
}

function halo(ctx: CanvasRenderingContext2D, color: string, radius: number, opacity: number): void {
  ctx.save();
  const glow = ctx.createRadialGradient(0, 0, 2, 0, 0, radius);
  glow.addColorStop(0, color);
  glow.addColorStop(1, "transparent");
  ctx.globalAlpha *= opacity;
  ctx.fillStyle = glow;
  ctx.beginPath();
  ctx.arc(0, 0, radius, 0, Math.PI * 2);
  ctx.fill();
  ctx.restore();
}

/** Рисует значок постоянного экранного размера после проекции координат карты. */
export function drawMissionMapMarker(
  ctx: CanvasRenderingContext2D,
  object: MissionObject,
  x: number,
  y: number,
  options: MissionMapMarkerOptions,
): void {
  ctx.save();
  try {
    ctx.translate(x, y);
    ctx.globalAlpha = 1;
    const localPlayer = options.local && object.kind === "avatar";
    const radius = missionMarkerRadius(options.size);
    if (options.target) halo(ctx, TARGET_COLOR, radius + 13, 0.33);
    if (localPlayer) halo(ctx, options.color, radius + 11, 0.4);

    ctx.fillStyle = "#121c22";
    ctx.beginPath();
    ctx.arc(0, 0, radius, 0, Math.PI * 2);
    ctx.fill();

    if (options.target || options.selected) {
      ctx.strokeStyle = options.target ? TARGET_COLOR : "#fff6e8";
      ctx.lineWidth = options.target ? 1.7 : 1.3;
      ctx.beginPath();
      ctx.arc(0, 0, radius + (options.selected ? 2.5 : 3), 0, Math.PI * 2);
      ctx.stroke();
    }

    if (localPlayer && Number.isFinite(options.heading)) ctx.rotate(options.heading);
    const scale = options.size / 24;
    ctx.scale(scale, scale);
    ctx.translate(-12, -12);
    ctx.lineJoin = "round";
    ctx.lineCap = "round";
    ctx.lineWidth = localPlayer ? 1.9 : 1.8;
    ctx.strokeStyle = options.color;
    ctx.fillStyle = object.kind === "avatar" ? options.color : "#121c22";
    const paths = pathsFor(object.kind);
    ctx.fill(paths.outline);
    ctx.stroke(paths.outline);
    if (paths.detail) {
      if (object.kind === "avatar") ctx.strokeStyle = "#121c22";
      ctx.stroke(paths.detail);
    }
  } finally {
    ctx.restore();
  }
}
