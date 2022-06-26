import { UiToEmu, EmuToUi, SaveType, saveTypes } from "../message";
import { FileId, Files, dbLookup } from "./files";
import { Input, Rect } from "./input";
import vertShaderSource from "raw-loader!../shaders/screen.vert";
import fragShaderSource from "raw-loader!../shaders/screen.frag";
import { isMobileBrowser } from "./utils";

export class Ui {
    private canvasContainer: HTMLElement;
    private canvas: HTMLCanvasElement;

    private input: Input;
    private audio: AudioContext;
    private audioTime: number;

    private exportSaveButton: HTMLButtonElement;
    private playButton: HTMLButtonElement;
    private stopButton: HTMLButtonElement;
    private resetButton: HTMLButtonElement;

    private limitFramerateCheckbox: HTMLInputElement;
    private touchControlsCheckbox: HTMLInputElement;

    private files: Files;
    private bios7?: Uint8Array;
    private bios9?: Uint8Array;
    private firmware?: Uint8Array;

    private gl: WebGLRenderingContext;
    private fbProgram: WebGLProgram;
    private fbCoordsAttrib: number;

    private worker: Worker | undefined;
    private rendererWorker: Worker | undefined;
    private gameTitle: string | undefined;
    private saveFilename: string | undefined;

    private nextRomFilename: string | undefined;
    private nextRomBuffer: Uint8Array | undefined;

    private playing: boolean;

    constructor(touch: boolean) {
        this.canvasContainer = document.getElementById(
            "canvas-container"
        ) as HTMLDivElement;
        this.canvas = document.getElementById("canvas") as HTMLCanvasElement;

        this.input = new Input(touch, this.pause.bind(this));
        this.audio = new (window.AudioContext ||
            (window as any).webkitAudioContext)();
        this.audioTime = 0;

        const startAudioContext = () => {
            this.audio.resume();
            document.removeEventListener("touchstart", startAudioContext);
            document.removeEventListener("touchend", startAudioContext);
        };
        document.addEventListener("touchstart", startAudioContext);
        document.addEventListener("touchend", startAudioContext);

        document.addEventListener("visibilitychange", () => {
            if (!this.worker) {
                return;
            }
            if (document.visibilityState === "visible") {
                this.play();
            } else {
                this.pause();
            }
        });

        this.exportSaveButton = document.getElementById(
            "export-save"
        ) as HTMLButtonElement;
        this.playButton = document.getElementById("play") as HTMLButtonElement;
        this.stopButton = document.getElementById("stop") as HTMLButtonElement;
        this.resetButton = document.getElementById(
            "reset"
        ) as HTMLButtonElement;

        this.limitFramerateCheckbox = document.getElementById(
            "toggle-framerate-limit"
        ) as HTMLInputElement;
        this.touchControlsCheckbox = document.getElementById(
            "toggle-touch-controls"
        ) as HTMLInputElement;

        this.files = new Files(
            (id, filename, buffer) => {
                switch (id) {
                    case FileId.Rom: {
                        this.queueRomLoad(filename, new Uint8Array(buffer));
                        break;
                    }
                    case FileId.Save: {
                        this.loadSave(filename, buffer);
                        break;
                    }
                    case FileId.Bios7: {
                        this.bios7 = new Uint8Array(buffer);
                        break;
                    }
                    case FileId.Bios9: {
                        this.bios9 = new Uint8Array(buffer);
                        break;
                    }
                    case FileId.Firmware: {
                        this.firmware = new Uint8Array(buffer);
                        break;
                    }
                }
            },
            () => {
                this.toggleRomEnabledIfSystemFilesLoaded();
            }
        );

        this.exportSaveButton.addEventListener(
            "click",
            this.requestSaveExport.bind(this)
        );
        this.playButton.addEventListener("click", this.play.bind(this));
        this.stopButton.addEventListener("click", this.requestStop.bind(this));
        this.resetButton.addEventListener("click", this.reset.bind(this));

        this.limitFramerateCheckbox.addEventListener("change", (e) => {
            this.setFramerateLimit(this.limitFramerateCheckbox.checked);
        });

        this.touchControlsCheckbox.checked = touch;
        this.touchControlsCheckbox.addEventListener("change", (e) => {
            this.input.touch = this.touchControlsCheckbox.checked;
        });

        const gl = this.canvas.getContext("webgl", {
            alpha: false,
            depth: false,
            stencil: false,
            antialias: false,
            powerPreference: "low-power",
        });
        if (!gl) {
            throw new Error("Couldn't create WebGL context");
        }
        this.gl = gl;

        const fbTexture = gl.createTexture()!;
        gl.bindTexture(gl.TEXTURE_2D, fbTexture);
        gl.texParameteri(gl.TEXTURE_2D, gl.TEXTURE_MIN_FILTER, gl.LINEAR);
        gl.texParameteri(gl.TEXTURE_2D, gl.TEXTURE_MAG_FILTER, gl.NEAREST);
        gl.texParameteri(gl.TEXTURE_2D, gl.TEXTURE_WRAP_S, gl.CLAMP_TO_EDGE);
        gl.texParameteri(gl.TEXTURE_2D, gl.TEXTURE_WRAP_T, gl.CLAMP_TO_EDGE);
        gl.texImage2D(
            gl.TEXTURE_2D,
            0,
            gl.RGBA,
            256,
            384,
            0,
            gl.RGBA,
            gl.UNSIGNED_BYTE,
            new Uint8Array(256 * 384 * 4)
        );

        const vertShader = gl.createShader(gl.VERTEX_SHADER)!;
        gl.shaderSource(vertShader, vertShaderSource);
        gl.compileShader(vertShader);
        if (!gl.getShaderParameter(vertShader, gl.COMPILE_STATUS)) {
            throw new Error(
                `WebGL vertex shader compilation failed: ${gl.getShaderInfoLog(
                    vertShader
                )}`
            );
        }

        const fragShader = gl.createShader(gl.FRAGMENT_SHADER)!;
        gl.shaderSource(fragShader, fragShaderSource);
        gl.compileShader(fragShader);
        if (!gl.getShaderParameter(fragShader, gl.COMPILE_STATUS)) {
            throw new Error(
                `WebGL fragment shader compilation failed: ${gl.getShaderInfoLog(
                    fragShader
                )}`
            );
        }

        const fbProgram = gl.createProgram()!;
        gl.attachShader(fbProgram, vertShader);
        gl.attachShader(fbProgram, fragShader);
        gl.linkProgram(fbProgram);
        if (!gl.getProgramParameter(fbProgram, gl.LINK_STATUS)) {
            throw new Error(
                `WebGL program linking failed: ${gl.getProgramInfoLog(
                    fbProgram
                )}`
            );
        }
        this.fbProgram = fbProgram;

        this.fbCoordsAttrib = gl.getAttribLocation(fbProgram, "coords");

        const fbBuffer = gl.createBuffer()!;
        gl.bindBuffer(gl.ARRAY_BUFFER, fbBuffer);
        gl.bufferData(
            gl.ARRAY_BUFFER,
            new Float32Array([-1, -1, 1, -1, -1, 1, 1, 1]),
            gl.STATIC_DRAW
        );

        this.playing = false;

        this.frame();

        window.addEventListener("beforeunload", () => {
            if (!this.worker) {
                return;
            }
            this.sendMessage({ type: UiToEmu.MessageType.Stop });
        });
    }

    toggleRomEnabledIfSystemFilesLoaded() {
        if (this.files?.gameDb) {
            this.files.toggleEnabled(FileId.Rom, true);
        }
    }

    sendMessage(message: UiToEmu.Message, transfer?: Transferable[]) {
        this.worker!.postMessage(message, transfer as any);
    }

    handleStartingWorkerMessage(e: MessageEvent) {
        const message = e.data as EmuToUi.Message;
        if (message.type !== EmuToUi.MessageType.Loaded) {
            return;
        }

        this.files.toggleEnabled(FileId.Save, true);
        this.exportSaveButton.disabled = false;
        this.playButton.disabled = false;
        this.stopButton.disabled = false;
        this.resetButton.disabled = false;
        this.limitFramerateCheckbox.disabled = false;
        this.limitFramerateCheckbox.checked = true;

        const romFilenameExtStart = this.nextRomFilename!.lastIndexOf(".");
        this.gameTitle =
            romFilenameExtStart === -1
                ? this.nextRomFilename!
                : this.nextRomFilename!.slice(0, romFilenameExtStart);
        this.saveFilename = `${this.gameTitle}.sav`;

        const gameCode = new Uint32Array(
            this.nextRomBuffer!.buffer,
            0,
            this.nextRomBuffer!.length >> 2
        )[0xc >> 2]!;
        let saveType: SaveType | undefined;
        const dbEntry = dbLookup(this.files.gameDb!, gameCode);
        if (dbEntry) {
            if (this.nextRomBuffer!.length !== dbEntry["rom-size"]) {
                console.warn(
                    `Unexpected ROM size: expected ${
                        dbEntry["rom-size"]
                    } B, got ${this.nextRomBuffer!.length} B`
                );
            }
            saveType = saveTypes[dbEntry["save-type"]];
        }
        this.sendMessage(
            {
                type: UiToEmu.MessageType.Start,
                rom: this.nextRomBuffer!,
                bios7: this.bios7,
                bios9: this.bios9,
                firmware: this.firmware,
                saveType,
                hasIR: (gameCode & 0xff) === 0x49,
            },
            [this.nextRomBuffer!.buffer]
        );

        this.files.loadSaveFromStorage(this.gameTitle);

        this.nextRomFilename = undefined;
        this.nextRomBuffer = undefined;
        this.worker!.onmessage = this.handleWorkerMessage.bind(this);
    }

    handleWorkerMessage(e: MessageEvent) {
        const message = e.data as EmuToUi.Message;
        switch (message.type) {
            case EmuToUi.MessageType.StartRenderer: {
                this.rendererWorker = new Worker("renderer_3d.bundle.js");
                this.rendererWorker.postMessage({
                    module: message.module,
                    memory: message.memory,
                });
                break;
            }

            case EmuToUi.MessageType.ExportSave: {
                this.files.storeSaveToStorage(
                    this.saveFilename!,
                    message.buffer,
                    this.gameTitle!
                );
                if (message.triggerDownload) {
                    const file = new Blob([message.buffer], {
                        type: "application/octet-stream;charset=utf-8",
                    });
                    const a = document.createElement("a");
                    const url = URL.createObjectURL(file);
                    a.href = url;
                    a.download = this.saveFilename!;
                    document.body.appendChild(a);
                    a.onclick = () => a.remove();
                    a.click();
                    URL.revokeObjectURL(url);
                }
                break;
            }

            case EmuToUi.MessageType.RenderFrame: {
                this.gl.texSubImage2D(
                    this.gl.TEXTURE_2D,
                    0,
                    0,
                    0,
                    256,
                    384,
                    this.gl.RGBA,
                    this.gl.UNSIGNED_BYTE,
                    new Uint8Array(message.buffer.buffer)
                );
                break;
            }

            case EmuToUi.MessageType.PlayAudioChunk: {
                const sysClockFreq = 1 << 25;
                const origFrameRate = sysClockFreq / (6.0 * 355.0 * 263.0);
                const inputSampleRate =
                    ((sysClockFreq / 1024.0) * 60.0) / origFrameRate;

                const currentTime =
                    this.audio.currentTime + (this.audio.baseLatency || 1 / 60);
                if (
                    this.audioTime >
                    currentTime + message.l.length / inputSampleRate
                ) {
                    break;
                }
                if (this.audioTime < currentTime) {
                    this.audioTime = currentTime;
                }
                const buffer = this.audio.createBuffer(
                    2,
                    message.l.length,
                    inputSampleRate
                );
                if (buffer.copyToChannel) {
                    buffer.copyToChannel(message.l, 0);
                    buffer.copyToChannel(message.r, 1);
                } else {
                    buffer.getChannelData(0).set(message.l);
                    buffer.getChannelData(1).set(message.r);
                }
                const src = this.audio.createBufferSource();
                src.buffer = buffer;
                src.connect(this.audio.destination);
                if (src.start) {
                    src.start(this.audioTime);
                } else if ((src as any).noteOn) {
                    (src as any).noteOn(this.audioTime);
                }
                this.audioTime += message.l.length / inputSampleRate;
                break;
            }
        }
    }

    handleClosingWorkerMessage(e: MessageEvent) {
        const message = e.data as EmuToUi.Message;
        if (message.type !== EmuToUi.MessageType.Stopped) {
            return;
        }

        this.worker = undefined;

        this.files.storeSaveToStorage(
            this.saveFilename!,
            message.buffer,
            this.gameTitle!
        );

        this.saveFilename = undefined;
        this.gameTitle = undefined;

        this.tryStartQueuedWorker();
    }

    requestStop() {
        if (!this.worker) return;

        this.files.toggleEnabled(FileId.Save, false);
        this.exportSaveButton.disabled = true;
        this.playButton.disabled = true;
        this.stopButton.disabled = true;
        this.resetButton.disabled = true;
        this.limitFramerateCheckbox.disabled = true;

        this.sendMessage({
            type: UiToEmu.MessageType.Stop,
        });
        this.worker.onmessage = this.handleClosingWorkerMessage.bind(this);

        this.files.unloadRom();
        this.gl.texSubImage2D(
            this.gl.TEXTURE_2D,
            0,
            0,
            0,
            256,
            384,
            this.gl.RGBA,
            this.gl.UNSIGNED_BYTE,
            new Uint8Array(256 * 384 * 4)
        );
    }

    tryStartQueuedWorker() {
        if (!this.nextRomFilename || !this.nextRomBuffer) {
            return;
        }
        this.worker = new Worker("emu.bundle.js");
        this.worker.onmessage = this.handleStartingWorkerMessage.bind(this);
    }

    queueRomLoad(filename: string, buffer: Uint8Array) {
        this.nextRomFilename = filename;
        this.nextRomBuffer = buffer;
        if (this.worker) {
            this.requestStop();
        } else {
            this.tryStartQueuedWorker();
        }
    }

    reset() {
        this.sendMessage({
            type: UiToEmu.MessageType.Reset,
        });
    }

    requestSaveExport() {
        this.sendMessage({
            type: UiToEmu.MessageType.ExportSave,
        });
    }

    setFramerateLimit(value: boolean) {
        this.sendMessage({
            type: UiToEmu.MessageType.UpdateFramerateLimit,
            value,
        });
    }

    loadSave(filename: string, buffer: ArrayBuffer) {
        this.files.storeSaveToStorage(filename, buffer, this.gameTitle!);
        this.saveFilename = filename;
        this.sendMessage(
            {
                type: UiToEmu.MessageType.LoadSave,
                buffer,
            },
            [buffer]
        );
    }

    frame() {
        if (this.playing) {
            const containerWidth = this.canvasContainer.clientWidth;
            const containerHeight = this.canvasContainer.clientHeight;
            const fbAspectRatio = 256 / 384;
            let width = Math.floor(
                Math.min(containerHeight * fbAspectRatio, containerWidth)
            );
            let height = Math.floor(width / fbAspectRatio);
            this.canvas.style.width = `${width}px`;
            this.canvas.style.height = `${height}px`;
            width *= window.devicePixelRatio;
            height *= window.devicePixelRatio;
            this.canvas.width = width;
            this.canvas.height = height;
            this.gl.viewport(0, 0, width, height);

            this.gl.clearColor(0, 0, 0, 1.0);
            this.gl.clear(this.gl.COLOR_BUFFER_BIT);
            this.gl.useProgram(this.fbProgram);
            this.gl.vertexAttribPointer(
                this.fbCoordsAttrib,
                2,
                this.gl.FLOAT,
                false,
                8,
                0
            );
            this.gl.enableVertexAttribArray(this.fbCoordsAttrib);
            this.gl.drawArrays(this.gl.TRIANGLE_STRIP, 0, 4);
        }

        const canvasRect = this.canvas.getBoundingClientRect();
        const changes = this.input.update(
            Rect.fromParts(
                canvasRect.bottom,
                canvasRect.left,
                canvasRect.width,
                canvasRect.height * 0.5
            )
        );
        if (this.worker && changes) {
            this.sendMessage({
                type: UiToEmu.MessageType.UpdateInput,
                ...changes,
            });
        }

        requestAnimationFrame(this.frame.bind(this));
    }

    play() {
        document.body.classList.remove("paused");
        this.playing = true;
        this.sendMessage({
            type: UiToEmu.MessageType.UpdatePlaying,
            value: true,
        });
    }

    pause() {
        document.body.classList.add("paused");
        this.playing = false;
        this.sendMessage({
            type: UiToEmu.MessageType.UpdatePlaying,
            value: false,
        });
    }
}

export const ui = new Ui(isMobileBrowser());
