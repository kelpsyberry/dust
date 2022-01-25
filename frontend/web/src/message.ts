export const enum SaveType {
    None = "none",
    Eeprom4k = "eeprom-4k",
    EepromFram64k = "eeprom-fram-64k",
    EepromFram512k = "eeprom-fram-512k",
    EepromFram1m = "eeprom-fram-1m",
    Flash2m = "flash-2m",
    Flash4m = "flash-4m",
    Flash8m = "flash-8m",
    Nand64m = "nand-64m",
    Nand128m = "nand-128m",
    Nand256m = "nand-256m",
}

export interface GameDbEntry {
    code: number;
    rom_size: number;
    save_type: SaveType;
}

export const enum InputBits {
    A = 1 << 0,
    B = 1 << 1,
    Select = 1 << 2,
    Start = 1 << 3,
    Right = 1 << 4,
    Left = 1 << 5,
    Up = 1 << 6,
    Down = 1 << 7,
    R = 1 << 8,
    L = 1 << 9,
    X = 1 << 16,
    Y = 1 << 17,
    Debug = 1 << 19,
}

export namespace UiToEmu {
    export const enum MessageType {
        Start,
        Reset,
        Stop,
        LoadSave,
        ExportSave,
        UpdateInput,
        UpdatePlaying,
        UpdateLimitFramerate,
    }

    export interface StartMessage {
        type: MessageType.Start;
        rom: Uint8Array;
        bios7: Uint8Array;
        bios9: Uint8Array;
        firmware: Uint8Array;
        gameDb: GameDbEntry[];
    }

    export interface RawMessage {
        type: MessageType.Reset | MessageType.ExportSave | MessageType.Stop;
    }

    export interface LoadSaveMessage {
        type: MessageType.LoadSave;
        buffer: Uint8Array;
    }

    export interface UpdateInputMessage {
        type: MessageType.UpdateInput;
        pressed: number;
        released: number;
        touchPos?: [number, number] | null;
    }

    export interface UpdateFlagMessage {
        type: MessageType.UpdatePlaying | MessageType.UpdateLimitFramerate;
        value: boolean;
    }

    export type Message =
        | StartMessage
        | RawMessage
        | LoadSaveMessage
        | UpdateInputMessage
        | UpdateFlagMessage;
}

export namespace EmuToUi {
    export const enum MessageType {
        ExportSave,
        RenderFrame,
    }

    export interface ExportSaveMessage {
        type: MessageType.ExportSave;
        buffer: Uint8Array;
    }

    export interface RenderFrameMessage {
        type: MessageType.RenderFrame;
        buffer: Uint32Array;
    }

    export type Message = ExportSaveMessage | RenderFrameMessage;
}
