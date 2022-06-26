import { UiToEmu, EmuToUi } from "../message";
import type * as wasm from "../../pkg";

function sendMessage(message: EmuToUi.Message, transfer?: Transferable[]) {
    postMessage(message, transfer as any);
}

(async () => {
    const wasm = await import("../../pkg");

    self.onmessage = async (e) => {
        const message = e.data;
        await wasm.default(message.module, message.memory);
        wasm.run_worker();
        close();
    };
})();
