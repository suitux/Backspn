import {
  BarController,
  BarElement,
  CategoryScale,
  Chart,
  Filler,
  Legend,
  LinearScale,
  LineController,
  LineElement,
  PointElement,
  Tooltip,
  type ChartDataset,
  type Plugin,
} from 'chart.js';
import type en from '../i18n/en';

Chart.register(LineController, LineElement, BarController, BarElement, PointElement, LinearScale, CategoryScale, Filler, Tooltip, Legend);

type Labels = (typeof en)['insights'];
type Range = '30d' | '90d' | '1y' | 'all';
type ChartId = keyof Labels['charts'];
type Tally = Record<string, number>;

/** Mirrors DailySummary in insights/src/summarize.ts. */
interface DailySummary {
  reporting: number;
  active: number;
  newInstalls: number;
  versions: Tally;
  platforms: Tally;
  controllers: Tally;
  librarySizes: Tally;
  fileExtensions: Tally;
  features: Tally;
  usage: Tally;
  fx: Tally;
  inputs: Tally;
}

interface SummaryResponse {
  range: Range;
  granularity: 'day' | 'week';
  /** When the live point was computed (ISO timestamp) */
  updatedAt: string | null;
  activeInstances: number;
  totals: { mixes: number; secondsPlaying: number; tracksPlayed: number; installs: number };
  history: { day: string; summary: DailySummary }[];
}

interface Series {
  label: string;
  values: number[];
  color: string;
}

/**
 * - line / bars: one value per date; stacked(-percent): bands that add up per date
 * - ranking: one horizontal bar per name (`names`), not over time
 */
interface ChartSpec {
  kind: 'line' | 'bars' | 'stacked' | 'stacked-percent' | 'ranking';
  series: Series[];
  percentAxis?: boolean;
  /** Per-thousand values, shown with one decimal */
  decimals?: boolean;
  names?: string[];
}

// Dark categorical palette, validated against the card surface (#18191d) with the dataviz
// validator: every adjacent pair clears CVD ΔE 8 and normal-vision ΔE 15, every slot clears 3:1.
// Assigned in this fixed order.
const CATEGORICAL = ['#3987e5', '#d95926', '#199e70', '#c98500', '#d55181', '#008300'];
// "Other" is not an entity, so it gets a neutral gray instead of a seventh hue.
const OTHER = '#6e6e6e';
// Library sizes are ordered buckets: one hue, dark → light.
const ORDINAL = ['#184f95', '#256abf', '#3987e5', '#6da7ec', '#9ec5f4', '#cde2fb'];

const SURFACE = '#18191d';
const GRID = 'rgba(242, 241, 238, 0.08)';
const MUTED = '#a3a2a0';
const FOREGROUND = '#f2f1ee';

const LIBRARY_BINS = ['0', '1-500', '500-2k', '2k-10k', '10k-50k', '50k+'];
const FEATURE_KEYS = ['controller', 'sync', 'hotcues', 'effects', 'loops', 'stems'] as const;
const TOP_N = 5;
const RANKED = 10;

/** Vertical hairline at the hovered X, so the reader aims at a date rather than a 2px line. */
const crosshair: Plugin<'line' | 'bar'> = {
  id: 'crosshair',
  afterDatasetsDraw(chart) {
    if ((chart.options as { indexAxis?: string }).indexAxis === 'y') return;
    const active = chart.tooltip?.getActiveElements();
    if (!active?.length) return;
    const { ctx, chartArea } = chart;
    const x = active[0].element.x;
    ctx.save();
    ctx.strokeStyle = 'rgba(242, 241, 238, 0.35)';
    ctx.lineWidth = 1;
    ctx.beginPath();
    ctx.moveTo(x, chartArea.top);
    ctx.lineTo(x, chartArea.bottom);
    ctx.stroke();
    ctx.restore();
  },
};

/** Numeric per part, so "0.0.10" follows "0.0.9"; a missing part counts as 0. */
function compareVersions(a: string, b: string): number {
  const aParts = a.split('.').map(Number);
  const bParts = b.split('.').map(Number);
  for (let i = 0; i < 3; i++) {
    const diff = (aParts[i] ?? 0) - (bParts[i] ?? 0);
    if (diff !== 0) return diff;
  }
  return 0;
}

/**
 * Keeps the TOP_N keys by their latest value and folds the rest into "Other". `order` decides
 * the slot each kept key gets, so a key keeps its color for as long as it stays in the view.
 */
function topSeries(maps: Tally[], otherLabel: string, order: (a: string, b: string) => number = (a, b) => a.localeCompare(b)): Series[] {
  const latest = maps.at(-1) ?? {};
  const keys = new Set(maps.flatMap((map) => Object.keys(map)));
  const ranked = [...keys].sort((a, b) => (latest[b] ?? 0) - (latest[a] ?? 0));
  const kept = ranked.slice(0, TOP_N).sort(order);
  const folded = ranked.slice(TOP_N);

  const series: Series[] = kept.map((key, index) => ({
    label: key,
    values: maps.map((map) => map[key] ?? 0),
    color: CATEGORICAL[index],
  }));

  if (folded.length > 0) {
    series.push({
      label: otherLabel,
      values: maps.map((map) => folded.reduce((sum, key) => sum + (map[key] ?? 0), 0)),
      color: OTHER,
    });
  }
  return series;
}

function toPercent(series: Series[]): Series[] {
  const totals = series[0]?.values.map((_, index) => series.reduce((sum, item) => sum + item.values[index], 0)) ?? [];
  return series.map((item) => ({
    ...item,
    values: item.values.map((value, index) => (totals[index] ? (value / totals[index]) * 100 : 0)),
  }));
}

/** The RANKED biggest entries of one tally, biggest first. */
function ranking(tally: Tally, label: string, rename: (key: string) => string = (key) => key): ChartSpec {
  const entries = Object.entries(tally)
    .filter(([, value]) => value > 0)
    .sort((a, b) => b[1] - a[1])
    .slice(0, RANKED);
  return {
    kind: 'ranking',
    names: entries.map(([key]) => rename(key)),
    series: [{ label, values: entries.map(([, value]) => value), color: CATEGORICAL[0] }],
  };
}

function sumTallies(maps: Tally[]): Tally {
  const sum: Tally = {};
  for (const map of maps) for (const [key, value] of Object.entries(map)) sum[key] = (sum[key] ?? 0) + value;
  return sum;
}

function buildSpecs(history: SummaryResponse['history'], labels: Labels): Record<ChartId, ChartSpec> {
  const summaries = history.map((point) => point.summary);
  const usage = (key: string) => summaries.map((summary) => summary.usage[key] ?? 0);
  const perThousand = (key: string) => summaries.map((summary) => (summary.usage.sessions ? ((summary.usage[key] ?? 0) / summary.usage.sessions) * 1000 : 0));

  return {
    mixes: { kind: 'bars', series: [{ label: labels.series.mixes, values: usage('mixes'), color: CATEGORICAL[0] }] },
    hours: { kind: 'line', series: [{ label: labels.series.hours, values: usage('secondsPlaying').map((s) => s / 3600), color: CATEGORICAL[0] }] },
    installs: {
      kind: 'line',
      series: [
        { label: labels.series.active, values: summaries.map((summary) => summary.active), color: CATEGORICAL[0] },
        { label: labels.series.reporting, values: summaries.map((summary) => summary.reporting), color: CATEGORICAL[1] },
      ],
    },
    versions: { kind: 'stacked', series: topSeries(summaries.map((summary) => summary.versions), labels.other, compareVersions) },
    platforms: { kind: 'stacked-percent', series: toPercent(topSeries(summaries.map((summary) => summary.platforms), labels.other)) },
    controllers: ranking(summaries.at(-1)?.controllers ?? {}, labels.charts.controllers.title, (key) => (key === 'none' ? labels.none : key)),
    effects: ranking(sumTallies(summaries.map((summary) => summary.fx)), labels.charts.effects.title),
    features: {
      kind: 'line',
      percentAxis: true,
      series: FEATURE_KEYS.map((key, index) => ({
        label: labels.features[key],
        values: summaries.map((summary) => (summary.active ? ((summary.features[key] ?? 0) / summary.active) * 100 : 0)),
        color: CATEGORICAL[index],
      })),
    },
    inputs: {
      kind: 'stacked-percent',
      series: toPercent(
        (['controller', 'keyboard'] as const).map((key, index) => ({
          label: labels.inputKinds[key],
          values: summaries.map((summary) => summary.inputs[key] ?? 0),
          color: CATEGORICAL[index],
        })),
      ),
    },
    librarySizes: {
      kind: 'stacked-percent',
      series: toPercent(
        LIBRARY_BINS.map((bin, index) => ({
          label: bin,
          values: summaries.map((summary) => summary.librarySizes[bin] ?? 0),
          color: ORDINAL[index],
        })),
      ),
    },
    formats: { kind: 'stacked-percent', series: toPercent(topSeries(summaries.map((summary) => summary.fileExtensions), labels.other)) },
    stability: {
      kind: 'line',
      decimals: true,
      series: (['crashes', 'uncleanExits'] as const).map((key, index) => ({
        label: labels.stability[key],
        values: perThousand(key),
        color: CATEGORICAL[index],
      })),
    },
  };
}

export function initInsightsDashboard(root: HTMLElement): void {
  const api = root.dataset.api!.replace(/\/$/, '');
  const locale = root.dataset.locale ?? 'en';
  const labels = JSON.parse(root.dataset.labels!) as Labels;

  const status = root.querySelector<HTMLElement>('.js-status')!;
  const content = root.querySelector<HTMLElement>('.js-content')!;
  const rangeButtons = [...root.querySelectorAll<HTMLButtonElement>('.js-range')];

  const integer = new Intl.NumberFormat(locale, { maximumFractionDigits: 0 });
  const compact = new Intl.NumberFormat(locale, { notation: 'compact', maximumFractionDigits: 1 });
  const oneDecimal = new Intl.NumberFormat(locale, { maximumFractionDigits: 1 });

  const charts = new Map<ChartId, Chart<'line' | 'bar', number[], string>>();
  let lastSpecs: Record<ChartId, ChartSpec> | null = null;
  let lastDates: string[] = [];
  let lastGranularity: SummaryResponse['granularity'] = 'day';

  function formatDate(day: string, withYear: boolean): string {
    return new Intl.DateTimeFormat(locale, {
      day: 'numeric',
      month: 'short',
      year: withYear ? 'numeric' : undefined,
      timeZone: 'UTC',
    }).format(new Date(`${day}T00:00:00Z`));
  }

  function formatValue(value: number, spec: ChartSpec): string {
    if (spec.kind === 'stacked-percent' || spec.percentAxis) return `${oneDecimal.format(value)} %`;
    return spec.decimals || (value > 0 && value < 10 && !Number.isInteger(value)) ? oneDecimal.format(value) : integer.format(value);
  }

  function setTile(id: string, value: string, note = '') {
    root.querySelector(`[data-tile="${id}"]`)!.textContent = value;
    root.querySelector(`[data-tile-note="${id}"]`)!.textContent = note;
  }

  function renderTiles(data: SummaryResponse | null) {
    if (!data) {
      for (const id of ['mixes', 'hours', 'tracks', 'active']) setTile(id, '–');
      return;
    }
    const inRange = (key: string, scale = 1) => {
      const sum = data.history.reduce((total, point) => total + (point.summary.usage[key] ?? 0), 0) / scale;
      return labels.inRange.replace('{value}', compact.format(sum));
    };
    setTile('mixes', integer.format(data.totals.mixes), inRange('mixes'));
    setTile('hours', integer.format(data.totals.secondsPlaying / 3600), inRange('secondsPlaying', 3600));
    setTile('tracks', compact.format(data.totals.tracksPlayed), inRange('tracksPlayed'));
    setTile('active', integer.format(data.activeInstances));
  }

  function datasetsFor(spec: ChartSpec): ChartDataset<'line' | 'bar', number[]>[] {
    if (spec.kind === 'bars' || spec.kind === 'ranking') {
      return spec.series.map((series) => ({
        type: 'bar' as const,
        label: series.label,
        data: series.values,
        backgroundColor: series.color,
        hoverBackgroundColor: series.color,
        borderRadius: 4,
        borderSkipped: 'start',
        // Thin bars with a surface gap between neighbours.
        maxBarThickness: spec.kind === 'ranking' ? 18 : 14,
        categoryPercentage: 0.8,
        barPercentage: 0.9,
      }));
    }
    const stacked = spec.kind !== 'line';
    return spec.series.map((series, index) => ({
      type: 'line' as const,
      label: series.label,
      data: series.values,
      borderColor: stacked ? SURFACE : series.color,
      backgroundColor: series.color,
      // Stacked bands are separated by a 2px surface gap, not by a border in another color.
      borderWidth: 2,
      fill: stacked ? (index === 0 ? 'origin' : '-1') : false,
      pointRadius: 0,
      pointHoverRadius: stacked ? 0 : 4,
      pointHoverBorderColor: SURFACE,
      pointHoverBorderWidth: 2,
      // Curved bands overshoot each other where a series drops to zero.
      tension: stacked ? 0 : 0.25,
    }));
  }

  function axisLabels(spec: ChartSpec, dates: string[], granularity: SummaryResponse['granularity']): string[] {
    if (spec.kind === 'ranking') return spec.names ?? [];
    return dates.map((day) => formatDate(day, granularity === 'week'));
  }

  function renderChart(id: ChartId, spec: ChartSpec, dates: string[], granularity: SummaryResponse['granularity']) {
    const chartLabels = axisLabels(spec, dates, granularity);
    const canvas = root.querySelector<HTMLCanvasElement>(`[data-chart="${id}"]`)!;
    const empty = root.querySelector<HTMLElement>(`[data-empty="${id}"]`);
    const hasData = spec.series.some((series) => series.values.some((value) => value > 0));
    empty?.toggleAttribute('hidden', hasData);
    canvas.toggleAttribute('hidden', !hasData);

    const existing = charts.get(id);
    if (existing) {
      existing.data.labels = chartLabels;
      existing.data.datasets = datasetsFor(spec);
      existing.update();
      return;
    }

    const ranked = spec.kind === 'ranking';
    const stacked = spec.kind === 'stacked' || spec.kind === 'stacked-percent';
    const isPercent = spec.kind === 'stacked-percent' || spec.percentAxis;
    const valueAxis = {
      stacked,
      beginAtZero: true,
      max: spec.kind === 'stacked-percent' ? 100 : undefined,
      grid: { color: GRID },
      border: { display: false },
      ticks: {
        color: MUTED,
        font: { size: 11 },
        maxTicksLimit: 5,
        callback: (value: string | number) => (isPercent ? `${value} %` : compact.format(Number(value))),
      },
    };
    const categoryAxis = {
      grid: { display: false },
      border: { color: GRID },
      ticks: { color: ranked ? FOREGROUND : MUTED, maxRotation: 0, autoSkip: !ranked, autoSkipPadding: 24, font: { size: ranked ? 12 : 11 } },
    };

    const chart = new Chart<'line' | 'bar', number[], string>(canvas, {
      type: spec.kind === 'bars' || ranked ? 'bar' : 'line',
      data: { labels: chartLabels, datasets: datasetsFor(spec) },
      plugins: [crosshair],
      options: {
        responsive: true,
        maintainAspectRatio: false,
        animation: false,
        indexAxis: ranked ? 'y' : 'x',
        interaction: { mode: 'index', intersect: false, axis: ranked ? 'y' : 'x' },
        scales: ranked ? { x: valueAxis, y: categoryAxis } : { x: categoryAxis, y: valueAxis },
        plugins: {
          legend: {
            // A single series is named by the chart title; two or more always get a legend.
            display: spec.series.length > 1,
            position: 'bottom',
            labels: {
              color: MUTED,
              boxWidth: 10,
              boxHeight: 10,
              padding: 14,
              font: { size: 12 },
              usePointStyle: !stacked,
              pointStyle: 'line',
            },
          },
          tooltip: {
            backgroundColor: '#26272c',
            borderColor: 'rgba(242, 241, 238, 0.12)',
            borderWidth: 1,
            titleColor: MUTED,
            bodyColor: FOREGROUND,
            padding: 10,
            boxWidth: 10,
            boxHeight: 2,
            usePointStyle: false,
            // Top of the stack first, matching the order the bands are drawn in.
            itemSort: stacked ? (a, b) => b.datasetIndex - a.datasetIndex : undefined,
            callbacks: {
              label: (item) => ` ${formatValue(Number(item.raw), lastSpecs![id])}  ${item.dataset.label}`,
              labelColor: (item) => {
                const color = lastSpecs![id].series[item.datasetIndex]?.color ?? MUTED;
                return { borderColor: color, backgroundColor: color };
              },
            },
          },
        },
      },
    });
    charts.set(id, chart);
  }

  function cell(row: HTMLTableRowElement, text: string, className: string) {
    const td = row.insertCell();
    td.className = className;
    td.textContent = text;
  }

  /** Table view: every value reachable without hovering. Built lazily, with textContent only. */
  function renderTable(details: HTMLDetailsElement) {
    const id = details.dataset.table as ChartId;
    const spec = lastSpecs?.[id];
    const container = details.querySelector('div')!;
    container.replaceChildren();
    if (!spec || !details.open) return;

    const ranked = spec.kind === 'ranking';
    const table = document.createElement('table');
    table.className = 'w-full text-left text-xs tabular-nums';
    const head = table.createTHead().insertRow();
    const first = ranked ? labels.name : lastGranularity === 'week' ? labels.week : labels.date;
    for (const text of [first, ...(ranked ? [labels.value] : spec.series.map((s) => s.label))]) {
      const th = document.createElement('th');
      th.className = 'sticky top-0 bg-raised py-1.5 pr-4 font-medium text-soft';
      th.textContent = text;
      head.appendChild(th);
    }

    const body = table.createTBody();
    const rows = ranked ? (spec.names ?? []).map((_, i) => i) : lastDates.map((_, i) => lastDates.length - 1 - i);
    // Newest first for dates, biggest first for rankings: the row people look for comes first.
    for (const index of rows) {
      const row = body.insertRow();
      row.className = 'border-t border-line';
      cell(row, ranked ? spec.names![index] : formatDate(lastDates[index], true), 'py-1.5 pr-4 text-soft');
      for (const series of spec.series) cell(row, formatValue(series.values[index], spec), 'py-1.5 pr-4');
    }
    container.appendChild(table);
  }

  const tables = [...root.querySelectorAll<HTMLDetailsElement>('.js-table')];
  for (const details of tables) {
    details.addEventListener('toggle', () => renderTable(details));
  }

  async function load(range: Range) {
    for (const button of rangeButtons) {
      button.setAttribute('aria-pressed', String(button.dataset.range === range));
    }
    // Refetch keeps the frame: the previous render stays, dimmed, until the new data lands.
    content.style.opacity = '0.5';

    try {
      const response = await fetch(`${api}/summary.json?range=${range}`);
      if (!response.ok) throw new Error(`HTTP ${response.status}`);
      const data = (await response.json()) as SummaryResponse;

      if (data.history.length === 0) {
        status.textContent = labels.empty;
        renderTiles(null);
        return;
      }

      status.textContent = data.updatedAt
        ? labels.updated.replace('{date}', new Intl.DateTimeFormat(locale, { dateStyle: 'medium', timeStyle: 'short' }).format(new Date(data.updatedAt)))
        : '';

      lastSpecs = buildSpecs(data.history, labels);
      lastDates = data.history.map((point) => point.day);
      lastGranularity = data.granularity;

      renderTiles(data);
      for (const [id, spec] of Object.entries(lastSpecs) as [ChartId, ChartSpec][]) {
        renderChart(id, spec, lastDates, data.granularity);
      }
      for (const details of tables) renderTable(details);
    } catch (error) {
      console.error('Could not load Backspn statistics:', error);
      status.textContent = labels.error;
    } finally {
      content.style.opacity = '';
    }
  }

  const initial = new URLSearchParams(location.search).get('range');
  const startRange: Range = initial === '30d' || initial === '1y' || initial === 'all' ? initial : '90d';

  for (const button of rangeButtons) {
    button.addEventListener('click', () => {
      const range = button.dataset.range as Range;
      const url = new URL(location.href);
      url.searchParams.set('range', range);
      history.replaceState(null, '', url);
      void load(range);
    });
  }

  void load(startRange);
}
