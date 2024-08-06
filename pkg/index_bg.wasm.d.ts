/* tslint:disable */
/* eslint-disable */
export function run_worker(): void;
export function __wbg_emustate_free(a: number): void;
export function emustate_reset(a: number): void;
export function emustate_load_save(a: number, b: number): void;
export function emustate_export_save(a: number): number;
export function emustate_update_input(a: number, b: number, c: number): void;
export function emustate_update_touch(a: number, b: number, c: number): void;
export function emustate_run_frame(a: number): number;
export function create_emu_state(a: number, b: number, c: number, d: number, e: number, f: number, g: number, h: number, i: number): number;
export function internal_get_module(): number;
export function internal_get_memory(): number;
export const memory: WebAssembly.Memory;
export function __wbindgen_free(a: number, b: number, c: number): void;
export function __wbindgen_malloc(a: number, b: number): number;
export function __wbindgen_realloc(a: number, b: number, c: number, d: number): number;
export function __wbindgen_exn_store(a: number): void;
export function __wbindgen_thread_destroy(a: number, b: number): void;
export function __wbindgen_start(): void;
