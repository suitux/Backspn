import type en from './en';

const es: typeof en = {
  lang: 'es',
  meta: {
    title: 'Backspn: software de DJ libre para cuatro decks',
    description:
      'Software de DJ libre y de código abierto para macOS, Windows y Linux. Mezcla cuatro decks con sync y keylock, separa stems en tu ordenador, usa 74 efectos a tiempo y tu controladora.',
  },
  nav: { features: 'Funciones', videos: 'Vídeos', faq: 'Preguntas', github: 'GitHub', otherLang: 'English' },
  hero: {
    title: 'Software de DJ para cuatro decks',
    lead: 'Mezcla con sync y keylock, separa cualquier pista en stems y lanza 74 efectos a tiempo. Libre y de código abierto, para macOS, Windows y Linux.',
    primary: 'Ver en GitHub',
    secondary: 'Ver el vídeo',
    windowLabel: 'La ventana de Backspn con dos decks sonando',
    facts: [
      { value: '4', label: 'decks' },
      { value: '74', label: 'efectos' },
      { value: '4', label: 'stems por pista' },
      { value: '26', label: 'controladoras' },
    ],
  },
  features: {
    title: 'Todo lo que pide una sesión',
    lead: 'Cada parte está pensada para la cabina: fácil de leer, fácil de alcanzar y firme en el beat.',
    items: {
      mixing: {
        title: 'Cuatro decks, un reloj',
        text: 'El sync ata cada deck al reloj maestro, también a mitad y al doble. El keylock mantiene el tono mientras cambias el tempo.',
        points: ['Ocho hotcues por deck', 'Loops y beat jump de 1/32 a 64 beats', 'Mixer de cuatro canales con EQ isolator, filtro y limitador'],
      },
      stems: {
        title: 'Stems de cualquier pista',
        text: 'Separa una pista en batería, bajo, resto y voz con tu propia CPU. Después silencia, filtra o manda a efectos cada parte mientras suena.',
        points: ['Sin GPU y sin conexión', 'El resultado se guarda para la próxima vez', 'Volumen, filtro y envío a FX por stem'],
      },
      effects: {
        title: '74 efectos a tiempo',
        text: 'Ecos, reverbs, filtros, rolls, frenos y el backspin que da nombre a la app, todos a tiempo con su deck.',
        points: ['Dos o cuatro unidades de FX', 'Modo grupo o individual', 'Routing insert, post-fader o envío'],
      },
      samples: {
        title: 'Sample decks y caja de ritmos',
        text: 'Carga one-shots y loops en cuatro huecos y lánzalos a tiempo. Programa patrones por pasos que suenan con el deck.',
        points: ['Los disparos caen en el beat', 'Los loops siguen el tempo del deck', 'Patrones por pasos en cada deck'],
      },
      library: {
        title: 'Una biblioteca que conoce tu música',
        text: 'Cada pista se analiza al cargarla: BPM, beatgrid, tonalidad y loudness. Encuéntrala rápido con búsqueda, playlists, smartlists y puntuaciones.',
        points: ['MP3, AAC, ALAC, FLAC, WAV, AIFF y Ogg', 'Columnas a tu gusto', 'Navegación con teclado'],
      },
      controllers: {
        title: 'Trae tu controladora',
        text: 'Siete controladoras tienen driver nativo con luces, aros y pantallas. Otras diecinueve tienen perfil MIDI, y el MIDI learn mapea el resto.',
        points: ['Drivers HID nativos', 'Mapeos de teclado', 'MIDI learn para cualquier aparato'],
      },
    },
    pending: 'Clip próximamente',
  },
  videos: {
    title: 'Míralo en marcha',
    lead: 'Grabado de la pantalla, con el sonido de la mezcla.',
    items: {
      overview: { title: 'Un recorrido por Backspn', text: 'Cargar, sync, efectos, stems y la biblioteca en pocos minutos.' },
      set: { title: 'Una mezcla a cuatro decks', text: 'Una sesión real de principio a fin.' },
      setup: { title: 'Primer arranque', text: 'Salida de audio, una controladora y tu música.' },
    },
    soon: 'Vídeo próximamente',
  },
  specs: {
    title: 'Por dentro',
    rows: [
      ['Sistema', 'macOS, Windows y Linux'],
      ['Audio', 'CoreAudio, WASAPI o ALSA, búfer de 256 frames'],
      ['Ficheros', 'MP3, AAC/M4A, ALAC, FLAC, WAV, AIFF, Ogg Vorbis'],
      ['Análisis', 'BPM, beatgrid, tonalidad y loudness EBU R128'],
      ['Stems', 'Batería, bajo, resto y voz, en la CPU'],
      ['Controladoras', '7 drivers nativos, 19 perfiles MIDI, MIDI learn'],
      ['Licencia', 'GPL-3.0 o posterior'],
    ],
  },
  open: {
    title: 'Libre y de código abierto',
    text: 'Backspn está escrito en Rust y publicado bajo la GPL. Lee el código, compílalo, avisa de un fallo o añade tu controladora.',
    action: 'Ver el código',
    code: 'Compílalo con Rust estable:',
  },
  faq: {
    title: 'Preguntas',
    items: [
      { q: '¿Cuánto cuesta?', a: 'Nada. Backspn es software libre bajo la GPL-3.0 o posterior.' },
      { q: '¿Puedo descargarlo?', a: 'Todavía no. Hasta la primera versión puedes compilarlo con Rust estable en un solo comando.' },
      { q: '¿En qué sistemas funciona?', a: 'En macOS, Windows y Linux. El audio sale por CoreAudio, WASAPI o ALSA con un búfer de 256 frames.' },
      { q: '¿Qué controladoras funcionan?', a: 'Siete controladoras tienen driver nativo, otras 19 tienen perfil MIDI y cualquier aparato MIDI se puede mapear con MIDI learn. Por ahora solo un driver nativo está probado con el aparato.' },
      { q: '¿Los stems necesitan GPU o conexión?', a: 'No. La separación corre en la CPU de tu ordenador, y cada resultado se guarda para la próxima vez.' },
    ],
  },
  cta: {
    title: 'Empieza a mezclar',
    text: 'El código y los pasos para compilar están en GitHub. Si tienes una controladora, tus pruebas ayudan mucho.',
    action: 'Ver en GitHub',
  },
  footer: {
    product: 'Producto',
    source: 'Código',
    language: 'Idioma',
    issues: 'Incidencias',
    licence: 'Licencia',
    copyright: '© 2026 Xavier Rincón. El nombre y el logotipo de Backspn no forman parte de la licencia del código.',
  },
};

export default es;
