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
export const exportReplay = (id: string, directory: string, online = false) =>
  invoke<string>('export_replay', { id, directory, online });
export type OnlineReplayStatus = {
  id: string;
  state: string;
  message: string;
  retryAfter: number | null;
};
export const connectOnlineReplays = (clientId: string, clientSecret: string) =>
  invoke<ApiConnection>('connect_online_replays', { credentials: { clientId, clientSecret } });
export type ApiConnection = {
  connected: boolean;
  saved: boolean;
  clientId: string;
  message: string;
};
export const disconnectOnlineReplays = () => invoke<ApiConnection>('disconnect_online_replays');
export const restoreApiConnection = () => invoke<ApiConnection>('restore_api_connection');
export const forgetApiConnection = () => invoke<ApiConnection>('forget_api_connection');
export const checkOnlineReplay = (id: string) =>
  invoke<OnlineReplayStatus>('check_online_replay', { id });

export type DownloadOptions = {
  songs: string;
  player: string | null;
};
export type DownloadStatus = {
  activeDownloads?: number;
  downloadedSets?: number;
  running: boolean;
  phase: string;
  message: string;
  total: number;
  completed: number;
  unavailable: number;
  pending: number;
  bytes: number;
  totalBytes: number | null;
  installedSets: number;
  updates: import('./model').Score[];
  issues: string[];
};
export const startDownloads = (options: DownloadOptions) =>
  invoke<void>('start_downloads', { options });
export const downloadStatus = () => invoke<DownloadStatus>('download_status');
export const pauseDownloads = () => invoke<void>('pause_downloads');
export type CalculationStatus = {
  running: boolean;
  processed: number;
  total: number;
  calculated: number;
  unavailable: number;
  message: string;
  updates: import('./model').Score[];
};
export const calculationStatus = () => invoke<CalculationStatus>('calculation_status');
export const startCalculations = () => invoke<void>('start_calculations');
export const selectCalculator = (calculator: string) =>
  invoke<Library>('select_calculator', { calculator });
export const savePlayerAccount = (account: import('./model').PlayerAccount) =>
  invoke<Library>('save_player_account', { account });
export const refreshPlayerIdentity = (id: number) =>
  invoke<{ accounts: import('./model').PlayerAccount[]; assignments: [string, number | null][] }>(
    'refresh_player_identity',
    { id },
  );

export const openApiRegistration = () => invoke<void>('open_osu_api_settings');

export const openScorePage = (link: { scoreId: string; mode: number; legacy: boolean }) =>
  invoke<void>('open_score_page', link);
