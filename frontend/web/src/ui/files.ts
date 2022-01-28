import { saveTypes } from "../message";

export const enum FileId {
    Rom = 1 << 0,
    Save = 1 << 1,
    Bios7 = 1 << 2,
    Bios9 = 1 << 3,
    Firmware = 1 << 4,
}

export interface GameDbEntry {
    code: number;
    ["rom-size"]: number;
    ["save-type"]: keyof typeof saveTypes;
}

export class FileInput {
    constructor(
        protected inputElement: HTMLInputElement,
        private loadCallback: (name: string, file: ArrayBuffer) => void,
        private storageKey?: string
    ) {
        inputElement.addEventListener("change", () => {
            const file = inputElement.files ? inputElement.files[0] : null;
            if (file) {
                this.loadFromInput(file);
            }
        });
    }

    get enabled(): boolean {
        return !this.inputElement.disabled;
    }

    set enabled(enabled: boolean) {
        this.inputElement.disabled = !enabled;
    }

    load(name: string, buffer: ArrayBuffer) {
        this.loadCallback(name, buffer);
    }

    unload() {}

    loadFromInput(file: File) {
        const reader = new FileReader();
        reader.onload = () => {
            const resultBuffer = reader.result as ArrayBuffer;
            if (this.storageKey) {
                reader.onload = () => {
                    this.load(file.name, resultBuffer);
                    this.storeDataURLToStorage(
                        file.name,
                        reader.result as string
                    );
                };
                reader.readAsDataURL(file);
            } else {
                this.load(file.name, resultBuffer);
            }
        };
        reader.readAsArrayBuffer(file);
    }

    loadFromStorage(storageKey?: string) {
        storageKey ??= this.storageKey;
        if (!storageKey) return;

        const base64 = localStorage[storageKey];
        if (base64) {
            const parts = base64.split(",");
            if (!parts[2]) {
                return;
            }
            const fileContents = atob(parts[2]);
            const buffer = new Uint8Array(fileContents.length);
            for (let j = fileContents.length; j--; ) {
                buffer[j] = fileContents.charCodeAt(j);
            }
            this.load(parts[0], buffer.buffer);
        }
    }

    storeDataURLToStorage(
        filename: string,
        dataURL: string,
        storageKey?: string
    ) {
        storageKey ??= this.storageKey;
        if (!storageKey) return;
        localStorage[storageKey] = filename + "," + dataURL;
    }

    storeToStorage(filename: string, data: ArrayBuffer, storageKey?: string) {
        storageKey ??= this.storageKey;
        if (!storageKey) return;
        let dataString = "";
        const data_ = new Uint8Array(data);
        for (let i = 0; i < data_.length; i++) {
            dataString += String.fromCharCode(data_[i]!);
        }
        this.storeDataURLToStorage(
            filename,
            "data:application/octet-stream;base64," + btoa(dataString),
            storageKey
        );
    }
}

export class FileInputWithIndicator extends FileInput {
    private labelElement: HTMLLabelElement;
    private fileNameElement: HTMLElement;
    private loadIndicatorUse: SVGUseElement;

    constructor(
        inputElement: HTMLInputElement,
        loadCallback: (name: string, file: ArrayBuffer) => void,
        storageKey?: string
    ) {
        super(inputElement, loadCallback, storageKey);
        this.labelElement = this.inputElement
            .nextElementSibling as HTMLLabelElement;
        this.fileNameElement = this.labelElement.getElementsByClassName(
            "file-name"
        )[0] as HTMLElement;
        this.loadIndicatorUse = this.labelElement.querySelector(
            ".load-indicator > use"
        ) as SVGUseElement;
    }

    override load(name: string, buffer: ArrayBuffer) {
        super.load(name, buffer);
        this.fileNameElement.textContent = name;
        this.loadIndicatorUse.setAttributeNS(
            "http://www.w3.org/1999/xlink",
            "xlink:href",
            "file-check.svg#icon"
        );
    }

    override unload(): void {
        super.unload();
        this.fileNameElement.textContent = "";
        this.loadIndicatorUse.setAttributeNS(
            "http://www.w3.org/1999/xlink",
            "xlink:href",
            "file-cross.svg#icon"
        );
    }
}

export class Files {
    private loadedFiles: number = 0;
    private fileInputs: Map<FileId, FileInput>;
    public gameDb?: GameDbEntry[];

    constructor(
        private loadFileCallback: (
            id: FileId,
            name: string,
            buffer: ArrayBuffer
        ) => void,
        gameDbLoadedCallback: () => void
    ) {
        const input = (
            type: {
                new (
                    inputElement: HTMLInputElement,
                    loadCallback: (name: string, file: ArrayBuffer) => void,
                    storageKey?: string
                ): FileInput;
            },
            id: FileId,
            elemId: string,
            storageKey?: string,
            markLoaded: boolean = true
        ): [FileId, FileInput] => {
            return [
                id,
                new type(
                    document.getElementById(elemId) as HTMLInputElement,
                    (name, buffer) => {
                        if (markLoaded) {
                            this.loadedFiles |= id;
                        }
                        this.loadFileCallback(id, name, buffer);
                    },
                    storageKey
                ),
            ];
        };
        this.fileInputs = new Map([
            input(FileInputWithIndicator, FileId.Rom, "rom-input"),
            input(
                FileInput,
                FileId.Save,
                "import-save-input",
                undefined,
                false
            ),
            input(FileInputWithIndicator, FileId.Bios7, "bios7-input", "bios7"),
            input(FileInputWithIndicator, FileId.Bios9, "bios9-input", "bios9"),
            input(FileInputWithIndicator, FileId.Firmware, "fw-input", "fw"),
        ]);
        for (const fileInput of this.fileInputs.values()) {
            fileInput.loadFromStorage();
        }

        fetch("resources/game_db.json")
            .then((r) => r.text())
            .then((db) => {
                this.gameDb = JSON.parse(db);
                gameDbLoadedCallback();
            });
    }

    loaded(id: FileId): boolean {
        return (this.loadedFiles & id) === id;
    }

    toggleEnabled(id: FileId, enabled: boolean) {
        this.fileInputs.get(id)!.enabled = enabled;
    }

    unloadRom() {
        this.loadedFiles &= ~FileId.Rom;
        this.fileInputs.get(FileId.Rom)!.unload();
    }

    loadSaveFromStorage(gameTitle: string) {
        this.fileInputs.get(FileId.Save)!.loadFromStorage(`save-${gameTitle}`);
    }

    storeSaveToStorage(filename: string, data: ArrayBuffer, gameTitle: string) {
        this.fileInputs
            .get(FileId.Save)!
            .storeToStorage(filename, data, `save-${gameTitle}`);
    }
}

export function dbLookup(
    db: GameDbEntry[],
    code: number
): GameDbEntry | undefined {
    let start = 0;
    let end = db.length - 1;
    while (start !== end) {
        const i = (start + end) >> 1;
        const possible = db[i]!;
        if (possible.code > code) {
            end = i;
        } else if (possible.code < code) {
            start = i + 1;
        } else {
            return possible;
        }
    }
    return undefined;
}
