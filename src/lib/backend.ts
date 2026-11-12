import { invoke } from '@tauri-apps/api/core';
import type { Library } from './model';
export const isDesktop = '__TAURI_INTERNALS__' in window;
export const loadLibrary = () => invoke<Library>('load_library');
export const importStable = (root: string, songs: string | null, force = false) =>
  invoke<Library>('import_stable', { root, songs, force });
export const artwork = (id: string) => invoke<string | null>('artwork', { id });
export const importLazer = (root: string, force = false) =>
  invoke<Library>('import_lazer', { root, force });
