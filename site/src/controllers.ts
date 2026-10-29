// Controllers with a native driver (crates/control/src/s4mk3.rs, kontrol/) or a MIDI
// profile (crates/control/profiles/). Names identify the hardware only.
export interface Brand {
  brand: string;
  models: string[];
}

// The only one tested on hardware so far.
export const tested = 'Kontrol S4 MK3';

export const native: Brand[] = [
  { brand: 'Native Instruments', models: [tested, 'Kontrol S4 MK2', 'Kontrol S2 MK3', 'Kontrol S2 MK2', 'Kontrol S3', 'Kontrol MX2', 'Kontrol Z1'] },
];

export const midi: Brand[] = [
  { brand: 'AlphaTheta', models: ['DDJ-FLX2', 'DDJ-FLX4', 'DDJ-GRV6'] },
  {
    brand: 'Pioneer DJ',
    models: [
      'DDJ-200', 'DDJ-400', 'DDJ-800', 'DDJ-1000', 'DDJ-ERGO', 'DDJ-FLX6', 'DDJ-RB', 'DDJ-REV5', 'DDJ-RR', 'DDJ-RX', 'DDJ-RZ', 'DDJ-RZX',
      'DDJ-SB', 'DDJ-SB2', 'DDJ-SP1', 'DDJ-SR', 'DDJ-SX', 'DDJ-SX2', 'DDJ-SZ', 'DDJ-WeGO', 'DDJ-WeGO2', 'DDJ-WeGO3', 'DDJ-WeGO4', 'DDJ-XP1', 'DDJ-XP2',
    ],
  },
  { brand: 'Denon DJ', models: ['MC7000'] },
  { brand: 'Hercules', models: ['DJControl Inpulse 200', 'DJControl Inpulse 300', 'DJControl Inpulse 500'] },
  { brand: 'Numark', models: ['Mixtrack Pro FX', 'Mixtrack Platinum FX'] },
  { brand: 'Reloop', models: ['Buddy', 'Ready'] },
];
