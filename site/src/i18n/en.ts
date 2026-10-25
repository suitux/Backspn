export default {
  lang: 'en',
  meta: {
    title: 'Backspn: free DJ software for four decks',
    description:
      'Free, open-source DJ software for macOS, Windows and Linux. Mix four decks with sync and keylock, split stems on your machine, play 74 beat-synced effects and use your controller.',
  },
  nav: { features: 'Features', videos: 'Videos', faq: 'Questions', github: 'GitHub', otherLang: 'Español' },
  hero: {
    title: 'DJ software for four decks',
    lead: 'Mix with sync and keylock, split any track into stems and play 74 effects on the beat. Free and open source, for macOS, Windows and Linux.',
    primary: 'View on GitHub',
    secondary: 'Watch the demo',
    windowLabel: 'The Backspn window with two decks playing',
    facts: [
      { value: '4', label: 'decks' },
      { value: '74', label: 'effects' },
      { value: '4', label: 'stems per track' },
      { value: '26', label: 'controllers' },
    ],
  },
  features: {
    title: 'Everything a set needs',
    lead: 'Each part was built for the booth: quick to read, quick to reach, steady on the beat.',
    items: {
      mixing: {
        title: 'Four decks, one clock',
        text: 'Sync locks every deck to the master clock, half and double time included. Keylock keeps the pitch while you change the tempo.',
        points: ['Eight hotcues per deck', 'Loops and beat jump from 1/32 to 64 beats', 'Four-channel mixer with isolator EQ, filter and limiter'],
      },
      stems: {
        title: 'Stems from any track',
        text: 'Split a track into drums, bass, other and vocals on your own CPU. Then mute, filter or send each one to the effects while it plays.',
        points: ['No GPU and no connection needed', 'The result is saved for the next time', 'Volume, filter and FX send per stem'],
      },
      effects: {
        title: '74 effects on the beat',
        text: 'Echoes, reverbs, filters, rolls, brakes and the backspin the app is named after, all timed to the deck they sit on.',
        points: ['Two or four FX units', 'Group or single mode', 'Insert, post-fader or send routing'],
      },
      samples: {
        title: 'Sample decks and a drum machine',
        text: 'Load one-shots and loops into four slots and fire them on the beat. Program step patterns that play in time with the deck.',
        points: ['Triggers land on the beat', 'Loops follow the deck tempo', 'Step patterns per deck'],
      },
      library: {
        title: 'A library that knows your music',
        text: 'Every track is analysed when it loads: BPM, beatgrid, key and loudness. Find it fast with search, playlists, smartlists and ratings.',
        points: ['MP3, AAC, ALAC, FLAC, WAV, AIFF and Ogg', 'Columns you choose', 'Keyboard navigation'],
      },
      controllers: {
        title: 'Bring your controller',
        text: 'Seven controllers have native drivers with lights, rings and screens. Nineteen more have MIDI profiles, and MIDI learn maps the rest.',
        points: ['Native HID drivers', 'Keyboard mappings', 'MIDI learn for any device'],
      },
    },
    pending: 'Clip coming soon',
  },
  videos: {
    title: 'See it working',
    lead: 'Recorded from the screen, with the sound of the mix.',
    items: {
      overview: { title: 'A tour of Backspn', text: 'Load, sync, effects, stems and the library in a few minutes.' },
      set: { title: 'A four-deck mix', text: 'A real set from start to finish.' },
      setup: { title: 'First start', text: 'Audio output, a controller and your music.' },
    },
    soon: 'Video coming soon',
  },
  specs: {
    title: 'Under the hood',
    rows: [
      ['System', 'macOS, Windows and Linux'],
      ['Audio', 'CoreAudio, WASAPI or ALSA, 256-frame buffer'],
      ['Files', 'MP3, AAC/M4A, ALAC, FLAC, WAV, AIFF, Ogg Vorbis'],
      ['Analysis', 'BPM, beatgrid, key and EBU R128 loudness'],
      ['Stems', 'Drums, bass, other and vocals, on the CPU'],
      ['Controllers', '7 native drivers, 19 MIDI profiles, MIDI learn'],
      ['Licence', 'GPL-3.0 or later'],
    ],
  },
  open: {
    title: 'Free and open source',
    text: 'Backspn is written in Rust and released under the GPL. Read the code, build it, report a bug or add your controller.',
    action: 'View the code',
    code: 'Build it with stable Rust:',
  },
  faq: {
    title: 'Questions',
    items: [
      { q: 'What does it cost?', a: 'Nothing. Backspn is free software under the GPL-3.0 or later.' },
      { q: 'Can I download it?', a: 'Not yet. Until the first release you can build it from source with stable Rust in one command.' },
      { q: 'Which systems does it run on?', a: 'macOS, Windows and Linux. Audio goes out through CoreAudio, WASAPI or ALSA with a 256-frame buffer.' },
      { q: 'Which controllers work?', a: 'Seven controllers have native drivers, 19 more have MIDI profiles, and any MIDI device can be mapped with MIDI learn. Only one native driver is tested on hardware so far.' },
      { q: 'Do stems need a GPU or a connection?', a: 'No. Separation runs on the CPU of your own machine, and each result is saved for the next time.' },
    ],
  },
  cta: {
    title: 'Start mixing',
    text: 'The source and the build steps are on GitHub. Testers with a controller are especially welcome.',
    action: 'View on GitHub',
  },
  footer: {
    product: 'Product',
    source: 'Source',
    language: 'Language',
    issues: 'Issues',
    licence: 'Licence',
    copyright: '© 2026 Xavier Rincón. The Backspn name and logo are not covered by the code licence.',
  },
};
