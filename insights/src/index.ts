import { Hono } from 'hono'
import { cors } from 'hono/cors'
import { reportSchema, type Report } from './schema'
import {
  ACTIVE_DAYS,
  excludeIncompleteDays,
  groupByWeek,
  installs,
  mergeReports,
  summarize,
  type DailySummary,
  type HistoryPoint
} from './summarize'

export interface Env {
  DB: D1Database
  ALLOWED_ORIGINS: string
}

const MAX_REPORT_BYTES = 32 * 1024
const REPORT_RETENTION_DAYS = 90
/** Days re-summarized on every run, so a late report still lands in its day. */
const SUMMARIZE_LOOKBACK_DAYS = 3
/** Short, so the dashboard follows new reports; the live query is cheap at this scale */
const LIVE_CACHE_SECONDS = 300

/** All-time totals the dashboard leads with, summed from the stored history plus today. */
const TOTALS = ['mixes', 'secondsPlaying', 'tracksPlayed', 'sessions', 'recordings', 'stemsSeparated'] as const
type Totals = Record<(typeof TOTALS)[number], number> & { installs: number }

const RANGES = {
  '30d': { days: 30, weekly: false },
  '90d': { days: 90, weekly: false },
  '1y': { days: 365, weekly: true },
  all: { days: null, weekly: true }
} as const

type Range = keyof typeof RANGES

function utcDay(date: Date): string {
  return date.toISOString().slice(0, 10)
}

function daysBefore(days: number, from: Date | string = new Date()): string {
  const date = typeof from === 'string' ? new Date(`${from}T00:00:00Z`) : new Date(from)
  date.setUTCDate(date.getUTCDate() - days)
  return utcDay(date)
}

const app = new Hono<{ Bindings: Env }>()

app.get('/', c => c.text('Backspn Insights. Dashboard: https://backspn.com/insights/'))

app.get('/healthz', c => c.text('ok'))

app.post('/collect', async c => {
  const declaredLength = Number(c.req.header('content-length') ?? 0)
  if (declaredLength > MAX_REPORT_BYTES) return c.body(null, 413)

  const body = await c.req.text()
  if (body.length > MAX_REPORT_BYTES) return c.body(null, 413)

  let json: unknown
  try {
    json = JSON.parse(body)
  } catch {
    return c.body(null, 400)
  }

  const parsed = reportSchema.safeParse(json)
  if (!parsed.success) return c.body(null, 400)

  const now = new Date()
  const day = utcDay(now)
  const db = c.env.DB
  const earlier = await db
    .prepare('SELECT data FROM reports WHERE instance_id = ?1 AND day = ?2')
    .bind(parsed.data.id, day)
    .first<{ data: string }>()
  const report = earlier ? mergeReports(JSON.parse(earlier.data) as Report, parsed.data) : parsed.data

  // No IP, no headers: only the validated report is stored.
  await db.batch([
    db
      .prepare(
        `INSERT INTO reports (instance_id, day, received_at, data) VALUES (?1, ?2, ?3, ?4)
         ON CONFLICT (instance_id, day) DO UPDATE SET received_at = excluded.received_at, data = excluded.data`
      )
      .bind(report.id, day, now.toISOString(), JSON.stringify(report)),
    db.prepare('INSERT OR IGNORE INTO instances (instance_id, first_seen) VALUES (?1, ?2)').bind(report.id, day)
  ])

  return c.body(null, 204)
})

app.use('/summary.json', async (c, next) => {
  const origins = c.env.ALLOWED_ORIGINS.split(',').map(origin => origin.trim())
  return cors({ origin: origins, allowMethods: ['GET'] })(c, next)
})

app.get('/summary.json', async c => {
  const rangeParam = c.req.query('range') ?? '90d'
  const range: Range = rangeParam in RANGES ? (rangeParam as Range) : '90d'
  const { days, weekly } = RANGES[range]
  const db = c.env.DB

  const query =
    days === null
      ? db.prepare('SELECT day, data FROM daily_summaries ORDER BY day')
      : db.prepare('SELECT day, data FROM daily_summaries WHERE day >= ?1 ORDER BY day').bind(daysBefore(days))

  const now = new Date()
  const today = utcDay(now)
  const [{ results }, live, stored] = await Promise.all([
    query.all<{ day: string; data: string }>(),
    computeDay(db, today),
    storedTotals(db, today)
  ])

  // The live point stands for today and replaces the stored row, which only exists if a late
  // re-summary wrote one. It is still filling up, so it skips the incomplete-day check.
  const closed: HistoryPoint[] = excludeIncompleteDays(
    results
      .filter(row => row.day !== today)
      .map(row => ({ day: row.day, summary: JSON.parse(row.data) as DailySummary }))
  )
  const daily = live ? [...closed, { day: today, summary: live }] : closed
  const history = weekly ? groupByWeek(daily) : daily
  const latest = daily.at(-1)

  const totals = { ...stored }
  if (live) for (const key of TOTALS) totals[key] += live.usage[key]

  c.header('Cache-Control', `public, max-age=${LIVE_CACHE_SECONDS}`)
  return c.json({
    range,
    granularity: weekly ? 'week' : 'day',
    updatedAt: latest ? now.toISOString() : null,
    activeInstances: latest?.summary.active ?? 0,
    totals,
    history
  })
})

async function storedTotals(db: D1Database, today: string): Promise<Totals> {
  const sums = TOTALS.map(key => `COALESCE(SUM(json_extract(data, '$.usage.${key}')), 0) AS ${key}`).join(', ')
  const [usage, seen] = await Promise.all([
    db.prepare(`SELECT ${sums} FROM daily_summaries WHERE day <> ?1`).bind(today).first<Record<string, number>>(),
    db.prepare('SELECT COUNT(*) AS n FROM instances').first<{ n: number }>()
  ])
  const totals = { installs: seen?.n ?? 0 } as Totals
  for (const key of TOTALS) totals[key] = usage?.[key] ?? 0
  return totals
}

/** One day's summary from the raw reports: that day's usage, and the installs active by then. */
export async function computeDay(db: D1Database, day: string): Promise<DailySummary | null> {
  const [{ results }, fresh] = await Promise.all([
    db
      .prepare('SELECT day, data FROM reports WHERE day > ?1 AND day <= ?2')
      .bind(daysBefore(ACTIVE_DAYS, day), day)
      .all<{ day: string; data: string }>(),
    db.prepare('SELECT COUNT(*) AS n FROM instances WHERE first_seen = ?1').bind(day).first<{ n: number }>()
  ])
  if (results.length === 0) return null

  const rows = results.map(row => ({ day: row.day, report: JSON.parse(row.data) as Report }))
  const sameDay = rows.filter(row => row.day === day).map(row => row.report)
  return summarize(sameDay, installs(rows), fresh?.n ?? 0)
}

export async function summarizeDay(db: D1Database, day: string): Promise<boolean> {
  const summary = await computeDay(db, day)
  if (!summary) return false

  await db
    .prepare(
      `INSERT INTO daily_summaries (day, data) VALUES (?1, ?2)
       ON CONFLICT (day) DO UPDATE SET data = excluded.data`
    )
    .bind(day, JSON.stringify(summary))
    .run()
  return true
}

export async function runDailyJob(db: D1Database, now = new Date()): Promise<void> {
  // Only days that already closed: today is still collecting.
  for (let offset = SUMMARIZE_LOOKBACK_DAYS; offset >= 1; offset--) {
    await summarizeDay(db, daysBefore(offset, now))
  }

  await db.prepare('DELETE FROM reports WHERE day < ?1').bind(daysBefore(REPORT_RETENTION_DAYS, now)).run()
}

export default {
  fetch: app.fetch,
  async scheduled(_controller: ScheduledController, env: Env, ctx: ExecutionContext) {
    ctx.waitUntil(runDailyJob(env.DB))
  }
} satisfies ExportedHandler<Env>
