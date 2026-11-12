// Reads an app-owned immutable snapshot, never the live game database.
const fs = require('node:fs');
const path = require('node:path');
const Realm = require('realm');
const { normalizeStatistics } = require('./statistics.cjs');
const [snapshot, root, output] = process.argv.slice(2);
let realm;
try {
  const version = Realm.schemaVersion(snapshot);
  realm = new Realm({ path: snapshot, readOnly: true, schemaVersion: version });
  const schema = realm.schema.find(s => s.name === 'Score');
  for (const field of ['ID','Date','Ruleset','Statistics','IsLegacyScore','Mods','BeatmapHash']) {
    if (!schema?.properties[field]) throw Error(`Unsupported lazer schema ${version}: missing ${field}`);
  }
  const replayOnly = process.argv[5] === '--replays';
  const bySha = replayOnly ? new Map() : new Map([...realm.objects('Beatmap')].map(m => [m.Hash, m]));
  const filePath = hash => /^[a-f0-9]{64}$/i.test(hash || '') ? path.join(root, 'files', hash[0], hash.slice(0,2), hash) : null;
  const rows = [];
  // Lazer snapshots can contain both legacy PascalCase and newer API-style snake_case enums.
  for (const s of realm.objects('Score')) {
    if (s.DeletePending) continue;
    if (replayOnly) {
      const replay = [...s.Files].find(f => f.Filename?.toLowerCase().endsWith('.osr'));
      rows.push({ id: `lazer:${s.ID}`, path: filePath(replay?.File?.Hash) });
      continue;
    }
    const m = s.BeatmapInfo && s.BeatmapInfo.Hash === s.BeatmapHash ? s.BeatmapInfo : bySha.get(s.BeatmapHash);
    const metadata = m?.Metadata;
    const background = [...(m?.BeatmapSet?.Files || [])].find(f => f.Filename?.replaceAll('\\','/').toLowerCase() === metadata?.BackgroundFile?.replaceAll('\\','/').toLowerCase());
    rows.push({
      id: s.ID.toString(), legacy: s.IsLegacyScore, legacyId: s.LegacyOnlineID > 0 ? String(s.LegacyOnlineID) : null,
      onlineId: s.OnlineID > 0 ? String(s.OnlineID) : null, playerId: s.User?.OnlineID > 1 ? s.User.OnlineID : null,
      player: s.User?.Username || 'Unknown player', date: s.Date.toISOString(), mode: s.Ruleset?.OnlineID,
      mapHash: m?.MD5Hash || '', mapSha: s.BeatmapHash || '', mapPath: filePath(m?.Hash), artwork: filePath(background?.File?.Hash),
      title: metadata?.TitleUnicode || metadata?.Title || 'Unmatched beatmap', artist: metadata?.ArtistUnicode || metadata?.Artist || '',
      difficulty: m?.DifficultyName || '', mapper: metadata?.Author?.Username || '',
      stats: normalizeStatistics(JSON.parse(s.Statistics || '{}'), s.Ruleset?.OnlineID), maximum: normalizeStatistics(JSON.parse(s.MaximumStatistics || '{}'), s.Ruleset?.OnlineID), mods: JSON.parse(s.Mods || '[]'),
      combo: s.MaxCombo, rawScore: s.TotalScore, legacyScore: s.LegacyTotalScore, accuracy: s.Accuracy * 100,
    });
  }
  fs.writeFileSync(output, JSON.stringify({ schema: version, rows }));
  realm.close();
  process.exit(0);
} catch (e) {
  try { realm?.close(); } catch {}
  console.error(e.message);
  process.exit(1);
}
