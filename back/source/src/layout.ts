/** 统一内容块网格旁的浮动面板位置。 */
export type AdjacentPanelPosition =
  | { side: "right"; top: number; left: number; width: number; height: number }
  | { side: "left"; top: number; right: number; width: number; height: number };

export interface AdjacentPanelLayout {
  columns?: number;
  gap?: number;
  panelColumns?: number;
  panelRows?: number;
  cardAspectRatio?: number;
}

const DEFAULT_LAYOUT = {
  columns: 6,
  gap: 16,
  panelColumns: 3,
  panelRows: 2,
  cardAspectRatio: 3 / 2
} satisfies Required<AdjacentPanelLayout>;

function resolveLayout(layout?: AdjacentPanelLayout): Required<AdjacentPanelLayout> {
  return { ...DEFAULT_LAYOUT, ...layout };
}

function gridColumnWidth(grid: HTMLDivElement, columns: number, gap: number): number {
  return (grid.offsetWidth - (columns - 1) * gap) / columns;
}

export function calculateAdjacentPanelPosition(
  card: HTMLElement,
  outer: HTMLDivElement,
  grid: HTMLDivElement,
  options?: AdjacentPanelLayout
): AdjacentPanelPosition {
  const { columns, gap, panelColumns, panelRows, cardAspectRatio } = resolveLayout(options);
  const columnWidth = gridColumnWidth(grid, columns, gap);
  const panelWidth = panelColumns * columnWidth + (panelColumns - 1) * gap;
  const panelHeight = panelRows * (columnWidth / cardAspectRatio) + (panelRows - 1) * gap;
  const cardLeft = card.offsetLeft;
  const cardTop = card.offsetTop;
  const cardWidth = card.offsetWidth;
  const cardCenter = cardLeft - grid.offsetLeft + cardWidth / 2;
  const columnIndex = Math.floor(cardCenter / (columnWidth + gap));

  return columnIndex < columns / 2
    ? { side: "right", top: cardTop, left: cardLeft + cardWidth + gap, width: panelWidth, height: panelHeight }
    : { side: "left", top: cardTop, right: outer.offsetWidth - cardLeft + gap, width: panelWidth, height: panelHeight };
}

export function defaultAdjacentPanelSize(
  grid: HTMLDivElement | null,
  options?: AdjacentPanelLayout
): { width: number; height: number } {
  if (!grid) return { width: 480, height: 320 };
  const { columns, gap, panelColumns, panelRows, cardAspectRatio } = resolveLayout(options);
  const columnWidth = gridColumnWidth(grid, columns, gap);
  return {
    width: panelColumns * columnWidth + (panelColumns - 1) * gap,
    height: panelRows * (columnWidth / cardAspectRatio) + (panelRows - 1) * gap
  };
}
