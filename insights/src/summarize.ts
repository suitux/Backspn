import type { Report, Tally, Usage } from './schema'

/** Library size buckets, by track count. Labels are what the dashboard shows. */
export const LIBRARY_BINS = [
  { label: '0', max: 0 },
  { label: '1-500', max: 500 },
  { label: '500-2k', max: 2_000 },
  { label: '2k-10k', max: 10_000 },
  { label: '10k-50k', max: 50_000 },
  { label: '50k+', max: Infinity }
] as const

/** Days of reports that make an install "active", and that its snapshot is taken from. */
export const ACTIVE_DAYS = 7

const TALLIES = ['inputs', 'fx', 'imports'] as const
type TallyKey = (typeof TALLIES)[number]
export type Counter = Exclude<keyof Usage, TallyKey>
export type Counters = Record<Counter, number>

export interface DailySummary {
  /** Installs that reported this day */
  reporting: number
  /** Installs with a report in the ACTIVE_DAYS up to this day */
  active: number
  /** Installs whose first report was this day */
  newInstalls: number
  /** The rest describe the active installs, from each one's latest report */
  versions: Tally
  platforms: Tally
  controllers: Tally
  librarySizes: Tally
  fileExtensions: Tally
  sampleRates: Tally
  bufferSizes: Tally
  decks: Tally
  locales: Tally
  /** Active installs that use each feature (configured, or used in the window) */
  features: Record<Feature, number>
  /** What happened this day, summed over every install */
  usage: Counters
  fx: Tally
  inputs: Tally
  imports: Tally
}

export const FEATURES = [
  'controller',
  'headphones',
  'fourDecks',
  'link',
  'externalMixer',
  'midiClock',
  'quantize',
  'sync',
  'loops',
  'hotcues',
  'recording',
  'stems',
  'sampler',
  'effects'
] as const
export type Feature = (typeof FEATURES)[number]

export interface HistoryPoint {
  day: string
  summary: DailySummary
}

/** An install's latest snapshot, with its usage summed over the reports in the window. */
export interface Install {
  report: Report
  usage: Usage
}

function increment(map: Tally, key: string, by = 1) {
  if (by === 0) return
  map[key] = (map[key] ?? 0) + by
}

function addTally(into: Tally, from: Tally) {
  for (const [key, n] of Object.entries(from)) increment(into, key, n)
}

function counterKeys(usage: Usage): Counter[] {
  return (Object.keys(usage) as (keyof Usage)[]).filter(
    (key): key is Counter => !(TALLIES as readonly string[]).includes(key)
  )
}

export function emptyUsage(): Usage {
  return {
    sessions: 0,
    secondsOpen: 0,
    secondsPlaying: 0,
    tracksLoaded: 0,
    tracksPlayed: 0,
    mixes: 0,
    recordings: 0,
    secondsRecording: 0,
    stemsSeparated: 0,
    syncs: 0,
    hotcues: 0,
    loops: 0,
    beatJumps: 0,
    samples: 0,
    autoFades: 0,
    linkSessions: 0,
    crashes: 0,
    uncleanExits: 0,
    inputs: {},
    fx: {},
    imports: {}
  }
}

function counters(usage: Usage): Counters {
  const { inputs: _i, fx: _f, imports: _m, ...rest } = usage
  return rest
}

export function addUsage(a: Usage, b: Usage): Usage {
  const sum = emptyUsage()
  for (const usage of [a, b]) {
    for (const key of counterKeys(usage)) sum[key] += usage[key]
    for (const key of TALLIES) addTally(sum[key], usage[key])
  }
  return sum
}

/** A second report the same day: the newer snapshot, and the usage of both. */
export function mergeReports(earlier: Report, later: Report): Report {
  return { ...later, usage: addUsage(earlier.usage, later.usage) }
}

export function libraryBin(tracks: number): string {
  return LIBRARY_BINS.find(bin => tracks <= bin.max)!.label
}

/** "0.4.2-beta.1" → "0.4": patch releases would split the chart into dozens of thin bands. */
export function minorVersion(version: string): string {
  const match = version.replace(/^v/, '').match(/^(\d+)\.(\d+)/)
  return match ? `${match[1]}.${match[2]}` : 'unknown'
}

export function installs(rows: { day: string; report: Report }[]): Install[] {
  const byId = new Map<string, { day: string; install: Install }>()
  for (const { day, report } of rows) {
    const seen = byId.get(report.id)
    if (!seen) {
      byId.set(report.id, { day, install: { report, usage: report.usage } })
      continue
    }
    seen.install.usage = addUsage(seen.install.usage, report.usage)
    if (day >= seen.day) {
      seen.day = day
      seen.install.report = report
    }
  }
  return [...byId.values()].map(entry => entry.install)
}

function uses(install: Install): Record<Feature, boolean> {
  const { report, usage } = install
  return {
    controller: report.controllers.length > 0,
    headphones: report.audio?.headphones ?? false,
    fourDecks: report.config.decks >= 4,
    link: report.config.link || usage.linkSessions > 0,
    externalMixer: report.config.externalMixer,
    midiClock: report.config.midiClock,
    quantize: report.config.quantize,
    sync: usage.syncs > 0,
    loops: usage.loops > 0,
    hotcues: usage.hotcues > 0,
    recording: usage.recordings > 0,
    stems: usage.stemsSeparated > 0,
    sampler: usage.samples > 0,
    effects: Object.keys(usage.fx).length > 0
  }
}

export function summarize(day: Report[], active: Install[], newInstalls: number): DailySummary {
  const summary: DailySummary = {
    reporting: day.length,
    active: active.length,
    newInstalls,
    versions: {},
    platforms: {},
    controllers: {},
    librarySizes: Object.fromEntries(LIBRARY_BINS.map(bin => [bin.label, 0])),
    fileExtensions: {},
    sampleRates: {},
    bufferSizes: {},
    decks: {},
    locales: {},
    features: Object.fromEntries(FEATURES.map(f => [f, 0])) as Record<Feature, number>,
    usage: counters(emptyUsage()),
    fx: {},
    inputs: {},
    imports: {}
  }

  for (const install of active) {
    const { report } = install
    increment(summary.versions, minorVersion(report.version))
    increment(summary.platforms, `${report.os.platform}/${report.os.arch}`)
    increment(summary.locales, report.locale || 'unknown')
    increment(summary.decks, String(report.config.decks))
    if (report.controllers.length === 0) increment(summary.controllers, 'none')
    for (const controller of new Set(report.controllers)) increment(summary.controllers, controller)
    if (report.library) {
      increment(summary.librarySizes, libraryBin(report.library.tracks))
      addTally(summary.fileExtensions, report.library.fileExtensions)
    }
    if (report.audio) {
      increment(summary.sampleRates, String(report.audio.sampleRate))
      increment(summary.bufferSizes, String(report.audio.bufferFrames ?? 'default'))
    }
    for (const [feature, used] of Object.entries(uses(install))) {
      if (used) summary.features[feature as Feature]++
    }
  }

  const total = day.reduce((sum, report) => addUsage(sum, report.usage), emptyUsage())
  summary.usage = counters(total)
  summary.fx = total.fx
  summary.inputs = total.inputs
  summary.imports = total.imports
  return summary
}

/**
 * Drops trailing days whose active count fell more than 20% from the day before: a day that
 * was summarized before every install had reported reads as a sudden drop (idea from Navidrome).
 */
export function excludeIncompleteDays(points: HistoryPoint[]): HistoryPoint[] {
  const result = [...points]
  while (result.length >= 2) {
    const last = result[result.length - 1].summary.active
    const previous = result[result.length - 2].summary.active
    if (last >= previous * 0.8) break
    result.pop()
  }
  return result
}

/** Monday of the ISO week, as YYYY-MM-DD. */
export function weekStart(day: string): string {
  const date = new Date(`${day}T00:00:00Z`)
  const offset = (date.getUTCDay() + 6) % 7
  date.setUTCDate(date.getUTCDate() - offset)
  return date.toISOString().slice(0, 10)
}

/**
 * One point per week for long ranges. Snapshots (active and reporting installs, versions…) keep
 * the week's last day; what happened (usage, new installs) adds the week's days up.
 */
export function groupByWeek(points: HistoryPoint[]): HistoryPoint[] {
  const byWeek = new Map<string, HistoryPoint>()
  for (const point of points) {
    const week = weekStart(point.day)
    const seen = byWeek.get(week)
    if (!seen) {
      byWeek.set(week, { day: week, summary: structuredClone(point.summary) })
      continue
    }
    const sum = seen.summary
    const next = structuredClone(point.summary)
    for (const key of Object.keys(next.usage) as Counter[]) next.usage[key] += sum.usage[key] ?? 0
    for (const key of TALLIES) addTally(next[key], sum[key])
    next.newInstalls += sum.newInstalls
    seen.summary = next
  }
  return [...byWeek.values()]
}
