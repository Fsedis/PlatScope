import { filterColor, objectFilterEntry, type MissionFilterIndex } from "./missionFilters";
import { drawMissionMapMarker } from "./missionMapMarkers";
import { missionMarkerSize, missionMarkerRadius, type MissionMapPreferences } from "./missionMapPreferences";
import {
  finitePosition, mapScale, scaledHeight, worldToCanvas, zoneInHeightSlice,
  type MapBounds, type MapCamera, type MissionObject, type MissionScene, type Position3,
} from "./missionResearch";

export interface MissionMapDrawState {
  liveAvatar: MissionObject | null;
  viewHeading: number | null | undefined;
  scene: MissionScene | null;
  bounds: MapBounds;
  camera: MapCamera;
  objects: MissionObject[];
  selectedKey: string;
  targetKey: string;
  rotateWithView: boolean;
  pickedKeys: string[];
  customIndex: MissionFilterIndex;
  markerPreferences: MissionMapPreferences;
  showZones: boolean;
  sliceEnabled: boolean;
  heightCenter: number;
  heightHalfWidth: number;
  width: number;
  height: number;
}

interface PreparedFace { vertices: number[]; minY: number; maxY: number; meanY: number; area: number; layer: number }
interface PreparedEdge { a: number; b: number; faces: number[] }
interface PreparedGeometry {
  vertices: Position3[];
  faces: PreparedFace[];
  edges: PreparedEdge[];
  minHeights: number[];
  maxHeights: number[];
}
interface SurfaceLayer { fill: Path2D; outline: Path2D; meanY: number; weight: number }
interface SurfacePaths { fill: Path2D; layers: SurfaceLayer[] }
interface MarkerPoint { object: MissionObject; x: number; y: number; size: number; radius: number; heightDirection: -1 | 0 | 1; important: boolean }
interface MapRect { x: number; y: number; width: number; height: number }

// Общие вершины сопоставляются по точным координатам: зазоры и недостающие участки не достраиваются.
function prepareGeometry(meshes: MissionScene["meshes"]): PreparedGeometry {
  const vertices: Position3[] = [], faces: PreparedFace[] = [];
  const vertexIds = new Map<string, number>(), edgeIndex = new Map<string, PreparedEdge>(), faceKeys = new Set<string>();
  let minY = Infinity, maxY = -Infinity;
  for (const mesh of meshes) {
    const ids = mesh.vertices.map(position => {
      if (!finitePosition(position)) return -1;
      const key = `${position[0]},${position[1]},${position[2]}`;
      let id = vertexIds.get(key);
      if (id === undefined) { id = vertices.length; vertices.push(position); vertexIds.set(key, id); }
      return id;
    });
    for (const face of mesh.faces) {
      if (face.length < 3 || face.some(index => !Number.isInteger(index) || index < 0 || ids[index] === undefined || ids[index] < 0)) continue;
      const polygon = face.map(index => ids[index]);
      const key = [...polygon].sort((a, b) => a - b).join(",");
      if (faceKeys.has(key)) continue;
      let area = 0, low = Infinity, high = -Infinity, sum = 0;
      for (let i = 0; i < polygon.length; i++) {
        const a = vertices[polygon[i]], b = vertices[polygon[(i + 1) % polygon.length]];
        area += a[0] * b[2] - b[0] * a[2];
        low = Math.min(low, a[1]); high = Math.max(high, a[1]); sum += a[1];
      }
      // Вертикальная грань в проекции не имеет площади и не задаёт силуэт поверхности.
      if (!Number.isFinite(area) || Math.abs(area) < 1e-10) continue;
      faceKeys.add(key);
      if (area < 0) polygon.reverse();
      const faceId = faces.length;
      faces.push({ vertices: polygon, minY: low, maxY: high, meanY: sum / polygon.length, area: Math.abs(area), layer: 0 });
      minY = Math.min(minY, low); maxY = Math.max(maxY, high);
      for (let i = 0; i < polygon.length; i++) {
        const a = polygon[i], b = polygon[(i + 1) % polygon.length];
        if (a === b) continue;
        const edgeKey = a < b ? `${a}:${b}` : `${b}:${a}`;
        const edge = edgeIndex.get(edgeKey);
        if (edge) { if (edge.faces.at(-1) !== faceId) edge.faces.push(faceId); }
        else edgeIndex.set(edgeKey, { a, b, faces: [faceId] });
      }
    }
  }
  const layerStep = Math.max(4, (maxY - minY) / 12);
  for (const face of faces) face.layer = Math.floor((face.meanY - minY) / layerStep);
  return {
    vertices, faces, edges: [...edgeIndex.values()],
    minHeights: [...new Set(faces.map(face => face.minY))].sort((a, b) => a - b),
    maxHeights: [...new Set(faces.map(face => face.maxY))].sort((a, b) => a - b),
  };
}

function heightRank(values: number[], value: number, inclusive: boolean): number {
  let low = 0, high = values.length;
  while (low < high) {
    const middle = (low + high) >>> 1;
    if (values[middle] < value || (inclusive && values[middle] === value)) low = middle + 1;
    else high = middle;
  }
  return low;
}

function sliceKey(geometry: PreparedGeometry, state: MissionMapDrawState): string {
  if (!state.sliceEnabled) return "all";
  // Между соседними высотами состав граней одинаков: движение игрока не пересобирает пути каждый кадр.
  return `${heightRank(geometry.minHeights, state.heightCenter + state.heightHalfWidth, true)}:${heightRank(geometry.maxHeights, state.heightCenter - state.heightHalfWidth, false)}`;
}

function appendFace(path: Path2D, face: PreparedFace, vertices: Position3[]) {
  face.vertices.forEach((id, index) => {
    const position = vertices[id];
    if (index) path.lineTo(position[0], position[2]); else path.moveTo(position[0], position[2]);
  });
  path.closePath();
}

function buildSurfaces(geometry: PreparedGeometry, state: MissionMapDrawState): SurfacePaths {
  const fill = new Path2D(), layers = new Map<number, SurfaceLayer>();
  const included = new Uint8Array(geometry.faces.length);
  geometry.faces.forEach((face, index) => {
    if (state.sliceEnabled && (face.minY > state.heightCenter + state.heightHalfWidth || face.maxY < state.heightCenter - state.heightHalfWidth)) return;
    included[index] = 1;
    let layer = layers.get(face.layer);
    if (!layer) { layer = { fill: new Path2D(), outline: new Path2D(), meanY: 0, weight: 0 }; layers.set(face.layer, layer); }
    appendFace(fill, face, geometry.vertices); appendFace(layer.fill, face, geometry.vertices);
    layer.meanY += face.meanY * face.area; layer.weight += face.area;
  });
  for (const edge of geometry.edges) {
    let visibleFace = -1, count = 0;
    for (const faceId of edge.faces) if (included[faceId]) { visibleFace = faceId; count++; }
    // Рёбра между соседними гранями скрыты; при срезе остаётся контур реально показанных граней.
    if (count !== 1) continue;
    const path = layers.get(geometry.faces[visibleFace].layer)!.outline;
    const a = geometry.vertices[edge.a], b = geometry.vertices[edge.b];
    path.moveTo(a[0], a[2]); path.lineTo(b[0], b[2]);
  }
  for (const layer of layers.values()) layer.meanY /= layer.weight;
  return { fill, layers: [...layers.values()] };
}

function drawBackground(ctx: CanvasRenderingContext2D, state: MissionMapDrawState, surfaces: SurfacePaths, scale: number, referenceHeight: number | null) {
  const { width, height } = state;
  ctx.fillStyle = "#111a23"; ctx.fillRect(0, 0, width, height);
  ctx.fillStyle = "#74939f16";
  for (let y = 18; y < height; y += 26) for (let x = 18; x < width; x += 26) ctx.fillRect(x, y, 1.2, 1.2);
  if (state.showZones) for (const zone of state.scene?.zones ?? []) {
    if (!zoneInHeightSlice(zone, state.sliceEnabled, state.heightCenter, state.heightHalfWidth)) continue;
    const corners: Position3[] = [[zone.min[0], zone.min[1], zone.min[2]], [zone.max[0], zone.min[1], zone.min[2]], [zone.max[0], zone.min[1], zone.max[2]], [zone.min[0], zone.min[1], zone.max[2]]];
    ctx.beginPath();
    corners.forEach((point, index) => { const [x, y] = worldToCanvas(point, state.camera, scale, width, height); if (index) ctx.lineTo(x, y); else ctx.moveTo(x, y); });
    ctx.closePath(); ctx.strokeStyle = state.scene?.zonesFresh === false ? "#8095a329" : "#9bbcca48";
    ctx.lineWidth = 1; ctx.setLineDash([5, 7]); ctx.stroke(); ctx.setLineDash([]);
  }
  ctx.save();
  // Порядок преобразований совпадает с worldToCanvas; выбор точек и карта остаются согласованы.
  ctx.translate(width / 2, height / 2); ctx.scale(scale, -scale); ctx.rotate(state.camera.angle ?? 0); ctx.translate(-state.camera.x, -state.camera.z);
  ctx.fillStyle = "#20343d"; ctx.fill(surfaces.fill);
  const orderedLayers = [...surfaces.layers].sort((a, b) => referenceHeight === null ? a.meanY - b.meanY : Math.abs(b.meanY - referenceHeight) - Math.abs(a.meanY - referenceHeight));
  for (const layer of orderedLayers) {
    const distance = referenceHeight === null ? 0 : Math.abs(layer.meanY - referenceHeight);
    const fade = 1 - Math.min(1, distance / 24) * .58;
    const light = 23 + scaledHeight(layer.meanY, state.bounds) * 5;
    ctx.globalAlpha = fade; ctx.fillStyle = `hsl(193 25% ${light}%)`; ctx.fill(layer.fill);
    ctx.strokeStyle = "#80afbf9c"; ctx.lineWidth = 1.15 / scale; ctx.lineJoin = "round"; ctx.lineCap = "round"; ctx.stroke(layer.outline);
  }
  ctx.restore();
}

function roundedRect(ctx: CanvasRenderingContext2D, rect: MapRect, radius: number) {
  const { x, y, width, height } = rect, r = Math.min(radius, width / 2, height / 2);
  ctx.beginPath(); ctx.moveTo(x + r, y); ctx.lineTo(x + width - r, y); ctx.quadraticCurveTo(x + width, y, x + width, y + r);
  ctx.lineTo(x + width, y + height - r); ctx.quadraticCurveTo(x + width, y + height, x + width - r, y + height);
  ctx.lineTo(x + r, y + height); ctx.quadraticCurveTo(x, y + height, x, y + height - r);
  ctx.lineTo(x, y + r); ctx.quadraticCurveTo(x, y, x + r, y); ctx.closePath();
}

function markerBounds(point: MarkerPoint, padding = 0): MapRect {
  const verticalRadius = Math.max(point.radius, point.heightDirection ? 8 : 0);
  return {
    x: point.x - point.radius - padding, y: point.y - verticalRadius - padding,
    width: point.radius * 2 + (point.heightDirection ? 16 : 0) + padding * 2,
    height: verticalRadius * 2 + padding * 2,
  };
}

function drawHeightIndicator(ctx: CanvasRenderingContext2D, point: MarkerPoint, color: string) {
  if (!point.heightDirection) return;
  const x = point.x + point.radius + 9, y = point.y;
  ctx.save();
  // Отметка рисуется в экранных координатах и не поворачивается вместе с картой.
  ctx.globalAlpha = 1;
  roundedRect(ctx, { x: x - 6, y: y - 8, width: 12, height: 16 }, 4);
  ctx.fillStyle = "#101b25"; ctx.fill();
  ctx.strokeStyle = color; ctx.lineWidth = 1; ctx.stroke();
  ctx.beginPath(); ctx.moveTo(x - 3, y + point.heightDirection * 2);
  ctx.lineTo(x, y - point.heightDirection * 2); ctx.lineTo(x + 3, y + point.heightDirection * 2);
  ctx.lineWidth = 1.8; ctx.lineJoin = "round"; ctx.lineCap = "round"; ctx.stroke();
  ctx.restore();
}

function drawScale(ctx: CanvasRenderingContext2D, width: number, height: number, scale: number) {
  if (width < 120 || height < 90) return;
  const wanted = Math.min(112, width * .25) / scale;
  const magnitude = 10 ** Math.floor(Math.log10(wanted));
  const metres = (wanted / magnitude >= 5 ? 5 : wanted / magnitude >= 2 ? 2 : 1) * magnitude;
  const pixels = metres * scale, x = 18, y = height - 21;
  const label = `${new Intl.NumberFormat("ru-RU", { maximumSignificantDigits: 3 }).format(metres)} м`;
  ctx.save(); ctx.font = "11px system-ui"; ctx.fillStyle = "#c2d2d9";
  ctx.shadowColor = "#111a23"; ctx.shadowBlur = 4; ctx.fillText(label, x, y - 9);
  ctx.beginPath(); ctx.moveTo(x, y - 3); ctx.lineTo(x, y + 3); ctx.moveTo(x, y); ctx.lineTo(x + pixels, y); ctx.moveTo(x + pixels, y - 3); ctx.lineTo(x + pixels, y + 3);
  ctx.strokeStyle = "#b7ced8"; ctx.lineWidth = 1.2; ctx.stroke(); ctx.restore();
}

export function createMissionMapRenderer(): (node: HTMLCanvasElement, state: MissionMapDrawState) => void {
  let meshSource: MissionScene["meshes"] | null = null, geometry: PreparedGeometry | undefined;
  let preparedSlice = "", surfaces: SurfacePaths | undefined;
  let background: HTMLCanvasElement | undefined, backgroundKey = "";
  let backgroundZones: MissionScene["zones"];
  return (node, state) => {
    const ctx = node.getContext("2d"); if (!ctx) return;
    const { width, height } = state;
    if (!Number.isFinite(width) || !Number.isFinite(height) || width <= 0 || height <= 0) return;
    const dpr = Math.min(window.devicePixelRatio || 1, 2);
    const pixelWidth = Math.max(1, Math.round(width * dpr)), pixelHeight = Math.max(1, Math.round(height * dpr));
    if (node.width !== pixelWidth) node.width = pixelWidth; if (node.height !== pixelHeight) node.height = pixelHeight;
    ctx.setTransform(dpr, 0, 0, dpr, 0, 0); ctx.clearRect(0, 0, width, height);
    if (!state.scene) { ctx.fillStyle = "#111a23"; ctx.fillRect(0, 0, width, height); return; }
    if (!geometry || meshSource !== state.scene.meshes) {
      geometry = prepareGeometry(state.scene.meshes); meshSource = state.scene.meshes; surfaces = undefined; backgroundKey = "";
    }
    const nextSlice = sliceKey(geometry, state);
    if (!surfaces || preparedSlice !== nextSlice) { surfaces = buildSurfaces(geometry, state); preparedSlice = nextSlice; backgroundKey = ""; }
    const scale = mapScale(state.bounds, width, height, state.camera.zoom);
    const mine = state.liveAvatar && state.liveAvatar.positionFresh !== false && finitePosition(state.liveAvatar.position) ? state.liveAvatar : null;
    const referenceHeight = mine && mine.positionFresh !== false ? Math.round(mine.position[1] / 2) * 2 : null;
    const zoneSlice = state.showZones && state.sliceEnabled ? `${state.heightCenter}:${state.heightHalfWidth}` : "all";
    const key = [width, height, dpr, state.camera.x, state.camera.z, state.camera.zoom, state.camera.angle ?? 0, scale, state.bounds.minY, state.bounds.maxY, state.showZones, state.scene.zonesFresh, zoneSlice, referenceHeight].join("|");
    if (!background || backgroundKey !== key || backgroundZones !== state.scene.zones) {
      background ??= document.createElement("canvas");
      if (background.width !== pixelWidth) background.width = pixelWidth; if (background.height !== pixelHeight) background.height = pixelHeight;
      const bg = background.getContext("2d"); if (!bg) return;
      bg.setTransform(dpr, 0, 0, dpr, 0, 0); drawBackground(bg, state, surfaces, scale, referenceHeight);
      backgroundKey = key; backgroundZones = state.scene.zones;
    }
    ctx.drawImage(background, 0, 0, width, height); drawScale(ctx, width, height, scale);
    const picked = new Set(state.pickedKeys), points: MarkerPoint[] = [];
    for (const raw of state.objects) {
      const object = mine && raw.key === mine.key ? mine : raw;
      if (!finitePosition(object.position)) continue;
      const [x, y] = worldToCanvas(object.position, state.camera, scale, width, height);
      const size = missionMarkerSize(object, state.customIndex, state.markerPreferences), radius = missionMarkerRadius(size);
      const difference = mine && object.positionFresh !== false ? object.position[1] - mine.position[1] : 0;
      const heightDirection = state.markerPreferences.showHeightIndicators !== false && !["avatar", "npc", "hostage"].includes(object.kind) && Math.abs(difference) >= 1 ? difference > 0 ? 1 : -1 : 0;
      const point: MarkerPoint = { object, x, y, size, radius, heightDirection, important: object.key === mine?.key || object.key === state.selectedKey || object.key === state.targetKey || picked.has(object.key) };
      const extent = markerBounds(point, object.key === state.targetKey ? 13 : object.key === mine?.key ? 11 : 4);
      if (extent.x + extent.width < 0 || extent.y + extent.height < 0 || extent.x > width || extent.y > height) continue;
      points.push(point);
    }
    const drawMarker = (point: MarkerPoint) => {
      const local = point.object.key === mine?.key;
      const heading = Number.isFinite(state.viewHeading) ? state.viewHeading! : state.camera.angle ?? 0;
      drawMissionMapMarker(ctx, point.object, point.x, point.y, {
        size: point.size,
        color: local ? objectFilterEntry(point.object, state.customIndex)?.color ?? "#71e7ed" : filterColor(point.object, state.customIndex),
        selected: point.object.key === state.selectedKey || picked.has(point.object.key), target: point.object.key === state.targetKey,
        local, heading: heading - (state.camera.angle ?? 0),
      });
    };
    for (const point of points) if (point.object.key !== mine?.key) drawMarker(point);
    const drawIndicator = (point: MarkerPoint) => drawHeightIndicator(ctx, point, point.object.key === state.targetKey ? "#edc581" : point.object.key === state.selectedKey || picked.has(point.object.key) ? "#fff6e8" : filterColor(point.object, state.customIndex));
    for (const point of points) if (!point.important) drawIndicator(point);
    // Отметки выбранных объектов и цели остаются поверх остальных точек.
    for (const point of points) if (point.important) drawIndicator(point);
    // Игрок остаётся поверх поверхностей и остальных значков.
    for (const point of points) if (point.object.key === mine?.key) drawMarker(point);
  };
}
