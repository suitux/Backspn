// Local-only: prints SQL that fills daily_summaries with ~14 months of fake, growing history,
// so the dashboard can be checked before real reports exist. Usage: pnpm seed:local
const DAYS = 420
const today = new Date()
const lines = ['DELETE FROM daily_summaries;']

const FX = ['Filter', 'Delay', 'Reverb', 'Echo', 'Flanger', 'Gater', 'BeatChopper', 'Phaser', 'LoFi', 'Ringmod', 'Tape']
const CONTROLLERS = {
  'kontrol-s4-mk3': 0.18,
  'pioneer-ddj-400': 0.14,
  'pioneer-ddj-flx4': 0.12,
  'traktor-kontrol-s2-mk3': 0.07,
  'numark-mixtrack-pro-fx': 0.05,
  'other-midi': 0.06,
  none: 0.38
}

for (let offset = DAYS; offset >= 1; offset--) {
  const date = new Date(today)
  date.setUTCDate(date.getUTCDate() - offset)
  const day = date.toISOString().slice(0, 10)

  const progress = (DAYS - offset) / DAYS
  const noise = 1 + Math.sin(offset * 1.7) * 0.06
  const weekend = [0, 5, 6].includes(date.getUTCDay()) ? 1.5 : 1
  const active = Math.round((15 + 1200 * progress ** 1.6) * noise)
  const reporting = Math.round(active * 0.45 * weekend)
  const share = value => Math.round(active * value)
  const per = value => Math.round(reporting * value * noise)

  const releases = Math.floor((DAYS - offset) / 60)
  const adoption = Math.min(0.85, ((DAYS - offset) % 60) / 20)
  const versions = releases === 0
    ? { '0.0': active }
    : { [`0.${releases}`]: share(adoption), [`0.${releases - 1}`]: active - share(adoption) }

  const mixes = per(14 * weekend)
  const summary = {
    reporting,
    active,
    newInstalls: Math.round(active * 0.02 * noise),
    versions,
    platforms: { 'macos/aarch64': share(0.46), 'windows/x86_64': share(0.38), 'macos/x86_64': share(0.06), 'linux/x86_64': share(0.1) },
    controllers: Object.fromEntries(Object.entries(CONTROLLERS).map(([k, v]) => [k, share(v)])),
    librarySizes: { '0': share(0.05), '1-500': share(0.22), '500-2k': share(0.33), '2k-10k': share(0.3), '10k-50k': share(0.09), '50k+': share(0.01) },
    fileExtensions: { mp3: share(1800), flac: share(700), wav: share(260), m4a: share(180), aiff: share(90) },
    sampleRates: { '44100': share(0.58), '48000': share(0.4), '96000': share(0.02) },
    bufferSizes: { '128': share(0.12), '256': share(0.44), '512': share(0.34), default: share(0.1) },
    decks: { '2': share(0.72), '4': share(0.28) },
    locales: { en: share(0.68), es: share(0.32) },
    features: {
      controller: share(0.62),
      headphones: share(0.55),
      fourDecks: share(0.28),
      link: share(0.04),
      externalMixer: share(0.06),
      midiClock: share(0.03),
      quantize: share(0.8),
      sync: share(0.7),
      loops: share(0.52),
      hotcues: share(0.61),
      recording: share(0.12 + 0.08 * progress),
      stems: share(0.05 + 0.25 * progress),
      sampler: share(0.15),
      effects: share(0.66)
    },
    usage: {
      sessions: per(1.6),
      secondsOpen: per(7200),
      secondsPlaying: per(5400 * weekend),
      tracksLoaded: per(22 * weekend),
      tracksPlayed: per(16 * weekend),
      mixes,
      recordings: per(0.15),
      secondsRecording: per(500),
      stemsSeparated: per(0.4 + 2 * progress),
      syncs: per(12),
      hotcues: per(30),
      loops: per(6),
      beatJumps: per(4),
      samples: per(3),
      autoFades: per(0.5),
      linkSessions: per(0.02),
      crashes: per(0.03 * (1 - progress * 0.7)),
      uncleanExits: per(0.05 * (1 - progress * 0.5))
    },
    fx: Object.fromEntries(FX.map((name, i) => [name, per(6 / (i + 1))])),
    inputs: { controller: per(700), keyboard: per(90) },
    imports: { nml: per(0.05), 'library-xml': per(0.03), folders: per(0.04) }
  }

  lines.push(`INSERT INTO daily_summaries (day, data) VALUES ('${day}', '${JSON.stringify(summary)}');`)
}

console.log(lines.join('\n'))
