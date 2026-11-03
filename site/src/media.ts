import { existsSync } from 'node:fs';
import { join } from 'node:path';

export interface Clip {
  file: string;
  record: string;
  /** Pixel size of the screenshot or clip, so the page reserves its space. */
  size?: [number, number];
  /** First frame of a looping clip, shown until it plays. */
  poster?: string;
}

export interface Video extends Clip {
  poster: string;
}

// Screenshots are copies of docs/images/ in the repo (the same ones the README shows).
// Clips in clips/ are short silent screen recordings of the app, each with its first
// frame as a .webp poster.
export const features = {
  mixing: { file: 'clips/mixing.mp4', poster: 'clips/mixing.webp', size: [1600, 866], record: 'Four synced decks playing while EQ and filter move.' },
  stems: { file: 'screens/stems.webp', size: [1125, 725], record: 'A stem deck with Drums, Bass, Other and Vocals lanes.' },
  effects: { file: 'clips/effects.mp4', poster: 'clips/effects.webp', size: [1600, 348], record: 'FX units switched on over a playing deck, dry/wet raised and the effect changed.' },
  samples: { file: 'screens/sample-decks.webp', size: [2000, 723], record: 'A track deck next to a sample deck with four slots.' },
  library: { file: 'clips/library.mp4', poster: 'clips/library.webp', size: [1600, 636], record: 'Two playing decks with the library below, the selection moving through it.' },
  controllers: { file: 'screens/s4-screens.webp', size: [1456, 380], record: 'The four screen layouts of the Kontrol S4 MK3.' },
} satisfies Record<string, Clip>;

export const gallery = {
  drumMachine: { file: 'clips/drum-machine.mp4', poster: 'clips/drum-machine.webp', size: [1350, 296], record: 'An FX unit in Drum Machine mode, steps added while it plays.' },
  kits: { file: 'screens/drum-machine-kits.webp', size: [1960, 1516], record: 'Preferences, Drum Machine page.' },
  patterns: { file: 'screens/drum-machine-patterns.webp', size: [1580, 528], record: 'One pattern with three step sizes.' },
  loopRecorder: { file: 'screens/loop-recorder.webp', size: [1220, 1156], record: 'The loop recorder above a deck.' },
  saveAsLoop: { file: 'screens/save-as-loop.webp', size: [1220, 544], record: 'Save as Loop on an active deck loop.' },
  themes: { file: 'clips/themes.mp4', poster: 'clips/themes.webp', size: [1600, 978], record: 'The window switching through the themes while two decks play.' },
} satisfies Record<string, Clip>;

export const videos = {
  overview: { file: 'videos/overview.mp4', poster: 'videos/overview.jpg', record: '2–3 minute tour: load tracks, mix two with sync, an effect, stems and the library.' },
  set: { file: 'videos/set.mp4', poster: 'videos/set.jpg', record: 'A real 5–10 minute four-deck mix, screen-recorded with the audio.' },
  setup: { file: 'videos/setup.mp4', poster: 'videos/setup.jpg', record: 'First launch: pick the audio output, connect a controller and import the music.' },
} satisfies Record<string, Video>;

export const hero: Clip = {
  file: 'clips/hero.mp4',
  poster: 'clips/hero.webp',
  size: [1920, 1174],
  record: 'The whole window mixing two synced decks: fader up, bass swapped (hero).',
};

const root = join(process.cwd(), 'public', 'media');
export const ready = (file: string) => existsSync(join(root, file));
/** A file in public/, under the site's base path. */
export const asset = (file: string) => `${import.meta.env.BASE_URL.replace(/\/$/, '')}/${file}`;
export const url = (file: string) => asset(`media/${file}`);
export const anyReady = (clips: Record<string, Clip>) => Object.values(clips).some((c) => ready(c.file));
