import { z } from 'zod'

/**
 * Mirrors `Report` in Backspn (crates/app/src/insights/report.rs) and the fixture
 * test/fixtures/report.json that a Rust test keeps in sync with it.
 * Unknown keys are stripped, so only what is listed here ever reaches the database.
 */

const count = z.number().int().nonnegative().max(1e9)
const key = z.string().min(1).max(32)
const short = z.string().max(32)

export const MAX_TALLY_KEYS = 100

const tally = z
  .record(key, count)
  .refine(value => Object.keys(value).length <= MAX_TALLY_KEYS, 'Too many keys')

/** Usage since the install's last accepted report. Missing counters (older apps) read as 0. */
export const usageSchema = z.object({
  sessions: count.default(0),
  secondsOpen: count.default(0),
  secondsPlaying: count.default(0),
  tracksLoaded: count.default(0),
  tracksPlayed: count.default(0),
  mixes: count.default(0),
  recordings: count.default(0),
  secondsRecording: count.default(0),
  stemsSeparated: count.default(0),
  syncs: count.default(0),
  hotcues: count.default(0),
  loops: count.default(0),
  beatJumps: count.default(0),
  samples: count.default(0),
  autoFades: count.default(0),
  linkSessions: count.default(0),
  crashes: count.default(0),
  uncleanExits: count.default(0),
  inputs: tally.default({}),
  fx: tally.default({}),
  imports: tally.default({})
})

export const reportSchema = z.object({
  schema: z.literal(1),
  id: z.uuid(),
  version: z.string().min(1).max(32),
  os: z.object({ platform: short, arch: short }),
  locale: z.string().max(8),
  audio: z
    .object({
      sampleRate: count,
      bufferFrames: count.nullable(),
      channels: count,
      headphones: z.boolean()
    })
    .nullable(),
  controllers: z.array(key).max(16),
  library: z
    .object({
      tracks: count,
      analyzed: count,
      playlists: count,
      fileExtensions: tally
    })
    .nullable(),
  config: z.object({
    decks: count,
    fxUnits: count,
    quantize: z.boolean(),
    snap: z.boolean(),
    autoGain: z.boolean(),
    externalMixer: z.boolean(),
    link: z.boolean(),
    midiClock: z.boolean()
  }),
  usage: usageSchema
})

export type Report = z.infer<typeof reportSchema>
export type Usage = z.infer<typeof usageSchema>
export type Tally = Record<string, number>
