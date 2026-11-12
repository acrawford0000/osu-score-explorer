import type { Score } from './model';
const maps = [
  ['xi', 'Blue Zenith', 'FOUR DIMENSIONS', 'Asphyxia'],
  ['Camellia', 'Exit This Earth’s Atomosphere', 'Galactic', 'rrtyui'],
  ['GYZE', 'HONESTY', 'DISHONEST', 'Bibbity Bill'],
  ['Imperial Circus Dead Decadence', 'Uta', 'Himei', 'Ponoyoshi'],
  ['Foreground Eclipse', 'Songs Compilation', 'I Won’t Fall', 'Seni'],
  ['SOUND HOLIC', 'Highway to Heaven', 'Extra', 'Sotarks'],
  ['UNDEAD CORPORATION', 'Everything will freeze', 'Time Freeze', 'Ekoro'],
  ['Kano', 'Stella-rium', 'Celestial', 'Reform'],
];
export function demoScores(): Score[] {
  return Array.from({ length: 96 }, (_, i) => {
    const y = 2017 + Math.floor(i / 10),
      map = maps[i % maps.length],
      stars = +(3.1 + (y - 2017) * 0.32 + (i % 7) * 0.19).toFixed(2),
      misses = i % 5 === 0 ? 1 : i % 4 === 0 ? 2 : 0,
      n100 = i % 13 === 0 ? 1 : 5 + (i % 24),
      fc = misses === 0 && i % 7 !== 0;
    const pp = +(stars * stars * 8.5 * (fc ? 1 : 0.73)).toFixed(2);
    return {
      id: `demo-${i}`,
      mapHash: `sample-${i % 8}`,
      player: 'Sample player',
      date: `${y}-${String((i % 12) + 1).padStart(2, '0')}-${String((i % 26) + 1).padStart(2, '0')}T12:00:00Z`,
      mode: 0,
      title: map[1],
      artist: map[0],
      difficulty: map[2],
      mapper: map[3],
      mods: [0, 8, 16, 64, 24][i % 5],
      n300: 800 - n100 - misses,
      n100,
      n50: 0,
      misses,
      combo: fc ? 1200 : 800 + (i % 250),
      maxCombo: 1200,
      rawScore: 10000000 + i * 12500,
      accuracy: (800 * 300 - n100 * 200 - misses * 300) / 2400,
      fc,
      ss: false,
      stars,
      pp,
      fcPp: fc ? pp : +(pp * 1.24).toFixed(2),
      source: y >= 2024 ? 'lazer' : 'stable',
      status: 'Illustrative sample values',
      hasArtwork: false,
    };
  });
}
