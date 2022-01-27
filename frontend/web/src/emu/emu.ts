import { UiToEmu, EmuToUi } from "../message";
import type * as wasm from "../../pkg";

function sendMessage(message: EmuToUi.Message, transfer?: Transferable[]) {
    postMessage(message, transfer as any);
}

class FpsLimiter {
    private limit_!: number | null;
    private timeout!: number;
    private expectedTimeoutTime!: number;
    private timeoutId: number | undefined;

    constructor(limit: number | null, public callback: () => void) {
        this.limit = limit;
    }

    get limit(): number | null {
        return this.limit_;
    }

    set limit(limit: number | null) {
        if (limit === this.limit_) {
            return;
        }
        this.limit_ = limit;

        clearTimeout(this.timeoutId);
        this.timeout = limit === null ? 0 : 1000 / limit;
        this.expectedTimeoutTime = this.expectedTimeoutTime
            ? this.expectedTimeoutTime + this.timeout
            : performance.now() + this.timeout;
        this.timeoutId = setTimeout(
            this.handleTimeout.bind(this),
            Math.max(0, this.expectedTimeoutTime - performance.now())
        );
    }

    handleTimeout() {
        this.callback();
        this.expectedTimeoutTime += this.timeout;
        this.timeoutId = setTimeout(
            this.handleTimeout.bind(this),
            Math.max(0, this.expectedTimeoutTime - performance.now())
        );
    }
}

(async () => {
    const wasm = await import("../../pkg");
    let playing = false;
    let fpsLimiter = new FpsLimiter(60, frame);
    let emu: wasm.EmuState | undefined;

    function frame() {
        if (!playing) return;
        const buffer = emu!.run_frame();
        sendMessage(
            {
                type: EmuToUi.MessageType.RenderFrame,
                buffer,
            },
            [buffer.buffer]
        );
    }

    self.onmessage = (e) => {
        const data = e.data as UiToEmu.Message;
        switch (data.type) {
            case UiToEmu.MessageType.Start: {
                emu = wasm.create_emu_state(
                    data.bios7,
                    data.bios9,
                    data.firmware,
                    data.rom,
                    undefined,
                    data.saveType as number | undefined,
                    data.hasIR,
                    wasm.WbgModel.Lite
                );
                break;
            }

            case UiToEmu.MessageType.Reset: {
                emu?.reset();
                break;
            }

            case UiToEmu.MessageType.Stop: {
                close();
                break;
            }

            case UiToEmu.MessageType.LoadSave: {
                emu?.load_save(new Uint8Array(data.buffer));
                break;
            }

            case UiToEmu.MessageType.ExportSave: {
                sendMessage({
                    type: EmuToUi.MessageType.ExportSave,
                    buffer: emu?.export_save() ?? new Uint8Array(0),
                });
                break;
            }

            case UiToEmu.MessageType.UpdateInput: {
                emu?.update_input(data.pressed, data.released);
                if (typeof data.touchPos !== "undefined") {
                    emu?.update_touch(data.touchPos?.[0], data.touchPos?.[1]);
                }
                break;
            }

            case UiToEmu.MessageType.UpdatePlaying: {
                playing = data.value;
                break;
            }

            case UiToEmu.MessageType.UpdateLimitFramerate: {
                fpsLimiter.limit = data.value ? 60.0 : null;
                break;
            }
        }
    };

    sendMessage({
        type: EmuToUi.MessageType.Loaded,
    });
})();
