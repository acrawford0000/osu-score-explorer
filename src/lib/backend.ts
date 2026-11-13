import { invoke, convertFileSrc } from '@tauri-apps/api/core';
export const audioTrack = async (id: string) => {
  const path = await invoke<string | null>('audio_track', { id });
  return path ? convertFileSrc(path) : null;
};
import type { Library, GameSource } from './model';
export const saveSources = (sources: GameSource[]) =>
  invoke<GameSource[]>('save_sources', { sources });
export const isDesktop = '__TAURI_INTERNALS__' in window;
export const loadLibrary = () => invoke<Library>('load_library');
export const importStable = (root: string, songs: string | null, force = false) =>
  invoke<Library>('import_stable', { root, songs, force });
export const artwork = (id: string) => invoke<string | null>('artwork', { id });
export const importLazer = (root: string, force = false) =>
  invoke<Library>('import_lazer', { root, force });
export const replayAvailability = (ids: string[]) =>
  invoke<{ available: { id: string; filename: string }[]; warning: string | null }>(
    'replay_availability',
    { ids },
  );
export const exportReplay = (id: string, directory: string) =>
  invoke<string>('export_replay', { id, directory });
