import cetus from "./assets/world-activity/cetus.png";
import vallis from "./assets/world-activity/vallis.png";
import cambion from "./assets/world-activity/cambion.png";
import zariman from "./assets/world-activity/zariman.png";
import duviri from "./assets/world-activity/duviri.png";
import baro from "./assets/world-activity/baro.png";
import teshin from "./assets/world-activity/teshin.png";
import varzia from "./assets/world-activity/varzia.png";
import events from "./assets/world-activity/events.png";
import warframe from "./assets/world-activity/warframe.png";
import relic from "./assets/world-activity/relic.png";
import lith from "./assets/world-activity/relic-lith.png";
import meso from "./assets/world-activity/relic-meso.png";
import neo from "./assets/world-activity/relic-neo.png";
import axi from "./assets/world-activity/relic-axi.png";

// Оригинальные текстуры Warframe. Источники и хеши — в assets/world-activity/sources.json.
// Эмблемы обозначают локацию, а текущее состояние цикла показывается отдельно текстом.
export const worldArtwork = {
  cetus: { src: cetus, mask: true },
  vallis: { src: vallis, mask: true },
  cambion: { src: cambion, mask: true },
  zariman: { src: zariman, mask: true },
  duviri: { src: duviri, mask: true },
  baro: { src: baro, mask: false },
  teshin: { src: teshin, mask: false },
  resurgence: { src: varzia, mask: false },
  events: { src: events, mask: true },
  warframe: { src: warframe, mask: true },
  relic: { src: relic, mask: true },
  lith: { src: lith, mask: false },
  meso: { src: meso, mask: false },
  neo: { src: neo, mask: false },
  axi: { src: axi, mask: false },
} as const;

export type WorldArtworkKind = keyof typeof worldArtwork;

export function relicArtwork(slug: string | null, englishName: string): WorldArtworkKind {
  const era = /^(lith|meso|neo|axi)(?:_|\s)/i.exec(slug || englishName)?.[1].toLowerCase();
  return era === "lith" || era === "meso" || era === "neo" || era === "axi" ? era : "relic";
}
