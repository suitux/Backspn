import fixture from './fixtures/report.json'
import { describe, expect, it } from 'vitest'
import { MAX_TALLY_KEYS, reportSchema, type Report } from '../src/schema'
import {
  addUsage,
  emptyUsage,
  excludeIncompleteDays,
  groupByWeek,
  installs,
  libraryBin,
  mergeReports,
  minorVersion,
  summarize,
  weekStart,
  type DailySummary
} from '../src/summarize'


function report(overrides: Partial<Report> = {}): Report {
  return { ...reportSchema.parse(fixture), id: crypto.randomUUID(), ...overrides }
}

function point(day: string, active: number, mixes = 0) {
  return {
    day,
    summary: { active, newInstalls: 1, reporting: 1, usage: { mixes }, fx: {}, inputs: {}, imports: {} } as unknown as DailySummary
  }
}

describe('reportSchema', () => {
  it('accepts the report Backspn sends', () => {
    expect(reportSchema.safeParse(fixture).success).toBe(true)
  })

  it('strips unknown keys', () => {
    const parsed = reportSchema.parse({ ...report(), ip: '1.2.3.4', path: '/Users/me/Music' })
    expect(parsed).not.toHaveProperty('ip')
    expect(parsed).not.toHaveProperty('path')
  })

  it('rejects a report without a valid id', () => {
    expect(reportSchema.safeParse({ ...report(), id: 'me@example.com' }).success).toBe(false)
  })

  it('rejects negative counts, long keys and too many keys', () => {
    const r = report()
    expect(reportSchema.safeParse({ ...r, usage: { ...r.usage, mixes: -1 } }).success).toBe(false)
    expect(reportSchema.safeParse({ ...r, controllers: ['x'.repeat(33)] }).success).toBe(false)
    const fx = Object.fromEntries(Array.from({ length: MAX_TALLY_KEYS + 1 }, (_, i) => [`fx${i}`, 1]))
    expect(reportSchema.safeParse({ ...r, usage: { ...r.usage, fx } }).success).toBe(false)
  })

  it('reads missing counters of an older app as zero', () => {
    const r = report()
    const { mixes: _, ...older } = r.usage
    expect(reportSchema.parse({ ...r, usage: older }).usage.mixes).toBe(0)
  })
})

describe('merging', () => {
  it('adds the usage of two reports of the same day and keeps the newer snapshot', () => {
    const first = report({ version: '0.0.3' })
    const second = { ...first, version: '0.0.4', usage: { ...emptyUsage(), mixes: 3, fx: { Filter: 1, Delay: 2 } } }
    const merged = mergeReports(first, second)
    expect(merged.version).toBe('0.0.4')
    expect(merged.usage.mixes).toBe(first.usage.mixes + 3)
    expect(merged.usage.fx).toEqual({ Filter: 13, Delay: 2 })
  })

  it('takes each install once, from its latest report, with its usage over the window', () => {
    const a = report({ version: '0.0.3' })
    const later = { ...a, version: '0.0.4' }
    const b = report()
    const list = installs([
      { day: '2026-10-21', report: later },
      { day: '2026-10-20', report: a },
      { day: '2026-10-21', report: b }
    ])
    expect(list).toHaveLength(2)
    const first = list.find(i => i.report.id === a.id)!
    expect(first.report.version).toBe('0.0.4')
    expect(first.usage.mixes).toBe(a.usage.mixes * 2)
  })
})

describe('summarize', () => {
  it('describes the active installs and sums the day', () => {
    const quiet = report({ controllers: [], config: { ...report().config, decks: 2 } })
    const busy = report()
    const summary = summarize([busy], installs([quiet, busy].map(r => ({ day: '2026-10-21', report: r }))), 1)
    expect(summary).toMatchObject({ reporting: 1, active: 2, newInstalls: 1 })
    expect(summary.controllers).toEqual({ none: 1, 'kontrol-s4-mk3': 1 })
    expect(summary.decks).toEqual({ '2': 1, '4': 1 })
    expect(summary.versions).toEqual({ '0.0': 2 })
    expect(summary.librarySizes['2k-10k']).toBe(2)
    expect(summary.features.controller).toBe(1)
    expect(summary.features.recording).toBe(2)
    expect(summary.usage.mixes).toBe(busy.usage.mixes)
    expect(summary.fx).toEqual({ Filter: 12 })
  })

  it('bins libraries and versions', () => {
    expect(libraryBin(0)).toBe('0')
    expect(libraryBin(500)).toBe('1-500')
    expect(libraryBin(60_000)).toBe('50k+')
    expect(minorVersion('v0.4.2-beta.1')).toBe('0.4')
    expect(minorVersion('dev')).toBe('unknown')
  })

  it('adds usage', () => {
    const sum = addUsage({ ...emptyUsage(), mixes: 1, inputs: { keyboard: 2 } }, { ...emptyUsage(), mixes: 2, inputs: { keyboard: 1, controller: 4 } })
    expect(sum.mixes).toBe(3)
    expect(sum.inputs).toEqual({ keyboard: 3, controller: 4 })
  })
})

describe('history', () => {
  it('drops trailing days with a sudden fall in active installs', () => {
    const points = [point('2026-10-01', 100), point('2026-10-02', 100), point('2026-10-03', 50)]
    expect(excludeIncompleteDays(points).map(p => p.day)).toEqual(['2026-10-01', '2026-10-02'])
  })

  it('groups weeks: snapshots from the last day, usage added up', () => {
    expect(weekStart('2026-10-25')).toBe('2026-10-19')
    const weeks = groupByWeek([point('2026-10-19', 10, 5), point('2026-10-20', 12, 7), point('2026-10-26', 13, 1)])
    expect(weeks.map(w => w.day)).toEqual(['2026-10-19', '2026-10-26'])
    expect(weeks[0].summary.active).toBe(12)
    expect(weeks[0].summary.usage.mixes).toBe(12)
    expect(weeks[0].summary.newInstalls).toBe(2)
  })
})
