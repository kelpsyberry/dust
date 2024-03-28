/* tslint:disable */
/* eslint-disable */
/**
*/
export function run_worker(): void;
/**
* @param {Uint8Array | undefined} arm7_bios_arr
* @param {Uint8Array | undefined} arm9_bios_arr
* @param {Uint8Array | undefined} firmware_arr
* @param {Uint8Array} rom_arr
* @param {Uint8Array | undefined} save_contents_arr
* @param {SaveType | undefined} save_type
* @param {boolean} has_ir
* @param {WbgModel} model
* @param {Function} audio_callback
* @returns {EmuState}
*/
export function create_emu_state(arm7_bios_arr: Uint8Array | undefined, arm9_bios_arr: Uint8Array | undefined, firmware_arr: Uint8Array | undefined, rom_arr: Uint8Array, save_contents_arr: Uint8Array | undefined, save_type: SaveType | undefined, has_ir: boolean, model: WbgModel, audio_callback: Function): EmuState;
/**
* @returns {any}
*/
export function internal_get_module(): any;
/**
* @returns {any}
*/
export function internal_get_memory(): any;
/**
*/
export enum SaveType {
  None = 0,
  Eeprom4k = 1,
  EepromFram64k = 2,
  EepromFram512k = 3,
  EepromFram1m = 4,
  Flash2m = 5,
  Flash4m = 6,
  Flash8m = 7,
  Nand64m = 8,
  Nand128m = 9,
  Nand256m = 10,
}
/**
*/
export enum WbgModel {
  Ds = 0,
  Lite = 1,
  Ique = 2,
  IqueLite = 3,
  Dsi = 4,
}
/**
*/
export class EmuState {
  free(): void;
/**
*/
  reset(): void;
/**
* @param {Uint8Array} ram_arr
*/
  load_save(ram_arr: Uint8Array): void;
/**
* @returns {Uint8Array}
*/
  export_save(): Uint8Array;
/**
* @param {number} pressed
* @param {number} released
*/
  update_input(pressed: number, released: number): void;
/**
* @param {number | undefined} [x]
* @param {number | undefined} [y]
*/
  update_touch(x?: number, y?: number): void;
/**
* @returns {Uint32Array}
*/
  run_frame(): Uint32Array;
}

export type InitInput = RequestInfo | URL | Response | BufferSource | WebAssembly.Module;

export interface InitOutput {
  readonly run_worker: () => void;
  readonly __wbg_emustate_free: (a: number) => void;
  readonly emustate_reset: (a: number) => void;
  readonly emustate_load_save: (a: number, b: number) => void;
  readonly emustate_export_save: (a: number) => number;
  readonly emustate_update_input: (a: number, b: number, c: number) => void;
  readonly emustate_update_touch: (a: number, b: number, c: number) => void;
  readonly emustate_run_frame: (a: number) => number;
  readonly create_emu_state: (a: number, b: number, c: number, d: number, e: number, f: number, g: number, h: number, i: number) => number;
  readonly internal_get_module: () => number;
  readonly internal_get_memory: () => number;
  readonly memory: WebAssembly.Memory;
  readonly __wbindgen_free: (a: number, b: number, c: number) => void;
  readonly __wbindgen_malloc: (a: number, b: number) => number;
  readonly __wbindgen_realloc: (a: number, b: number, c: number, d: number) => number;
  readonly __wbindgen_exn_store: (a: number) => void;
  readonly __wbindgen_thread_destroy: (a?: number, b?: number) => void;
  readonly __wbindgen_start: () => void;
}

export type SyncInitInput = BufferSource | WebAssembly.Module;
/**
* Instantiates the given `module`, which can either be bytes or
* a precompiled `WebAssembly.Module`.
*
* @param {SyncInitInput} module
* @param {WebAssembly.Memory} maybe_memory
*
* @returns {InitOutput}
*/
export function initSync(module: SyncInitInput, maybe_memory?: WebAssembly.Memory): InitOutput;

/**
* If `module_or_path` is {RequestInfo} or {URL}, makes a request and
* for everything else, calls `WebAssembly.instantiate` directly.
*
* @param {InitInput | Promise<InitInput>} module_or_path
* @param {WebAssembly.Memory} maybe_memory
*
* @returns {Promise<InitOutput>}
*/
export default function __wbg_init (module_or_path?: InitInput | Promise<InitInput>, maybe_memory?: WebAssembly.Memory): Promise<InitOutput>;
