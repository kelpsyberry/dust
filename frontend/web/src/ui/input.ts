import { InputBits } from "../message";
import { TouchControls, Touch } from "./touch_controls";

const keyToInputBit: { [key: string]: number } = {
    w: InputBits.R,
    q: InputBits.L,
    a: InputBits.Y,
    s: InputBits.X,
    z: InputBits.B,
    x: InputBits.A,
    Enter: InputBits.Start,
    Shift: InputBits.Select,
    ArrowRight: InputBits.Right,
    ArrowLeft: InputBits.Left,
    ArrowDown: InputBits.Down,
    ArrowUp: InputBits.Up,
};

export interface InputChanges {
    pressed: number;
    released: number;
}

export class Input {
    public controls: HTMLElement;
    private pauseButtonContainer: HTMLElement;
    private pauseButton: HTMLButtonElement;

    private touch_!: boolean;
    public touchControls?: TouchControls;
    private touches: Map<number, Touch> = new Map();
    private touchStartCallback: (e: TouchEvent) => void;
    private touchMoveCallback: (e: TouchEvent) => void;
    private touchEndCallback: (e: TouchEvent) => void;

    private pressedKeys: number;

    constructor(touch: boolean, private pauseCallback: () => void) {
        this.controls = document.getElementById("controls")!;
        this.pauseButtonContainer = document.getElementById("btn-pause")!;
        this.pauseButton = this.pauseButtonContainer.getElementsByTagName(
            "button"
        )[0] as HTMLButtonElement;

        this.touchStartCallback = this.touchStart.bind(this);
        this.touchMoveCallback = this.touchMove.bind(this);
        this.touchEndCallback = this.touchEnd.bind(this);

        this.pressedKeys = 0;

        document.body.addEventListener("keydown", (e) => {
            this.pressedKeys |= keyToInputBit[e.key] ?? 0;
        });
        document.body.addEventListener("keyup", (e) => {
            this.pressedKeys &= ~(keyToInputBit[e.key] ?? 0);
        });

        this.touch = touch;
    }

    get touch(): boolean {
        return this.touch_;
    }

    set touch(touch: boolean) {
        if (touch === this.touch_) {
            return;
        }
        this.touch_ = touch;
        this.controls.classList.toggle("touch", touch);
        if (touch) {
            this.pauseButton.removeEventListener("click", this.pauseCallback);

            this.touchControls = new TouchControls(this.controls);
            this.touchControls.layoutData = this.touchControls.defaultLayout; // TODO: Remove

            this.controls.addEventListener(
                "touchstart",
                this.touchStartCallback
            );
            this.controls.addEventListener("touchmove", this.touchMoveCallback);
            window.addEventListener("touchend", this.touchEndCallback);
            window.addEventListener("touchcancel", this.touchEndCallback);

            this.touchControls.pause.interactionElement.addEventListener(
                "click",
                this.pauseCallback
            );
        } else {
            if (this.touchControls) {
                this.touchControls.pause.interactionElement.removeEventListener(
                    "click",
                    this.pauseCallback
                );

                delete this.touchControls;

                this.controls.removeEventListener(
                    "touchstart",
                    this.touchStartCallback
                );
                this.controls.removeEventListener(
                    "touchmove",
                    this.touchMoveCallback
                );
                window.removeEventListener("touchend", this.touchEndCallback);
                window.removeEventListener(
                    "touchcancel",
                    this.touchEndCallback
                );
            }

            this.pauseButton.addEventListener("click", this.pauseCallback);
        }
    }

    touchStart(e: TouchEvent) {
        if (!this.pauseButtonContainer.contains(e.target as Node)) {
            e.preventDefault();
        }
        for (let i = 0; i < e.changedTouches.length; ++i) {
            const t = e.changedTouches[i]!;
            this.touches.set(t.identifier, {
                startX: t.clientX,
                startY: t.clientY,
                x: t.clientX,
                y: t.clientY,
            });
        }
    }

    touchMove(e: TouchEvent) {
        if (!this.pauseButtonContainer.contains(e.target as Node)) {
            e.preventDefault();
        }
        for (let i = 0; i < e.changedTouches.length; ++i) {
            const t = e.changedTouches[i]!;
            const touch = this.touches.get(t.identifier);
            if (touch) {
                touch.x = t.clientX;
                touch.y = t.clientY;
            }
        }
    }

    touchEnd(e: TouchEvent) {
        for (let i = 0; i < e.changedTouches.length; ++i) {
            const t = e.changedTouches[i]!;
            this.touches.delete(t.identifier);
        }
    }

    process(): number {
        let input = this.pressedKeys;
        if (this.touchControls) {
            this.touchControls.resetTouches();
            for (const touch of this.touches.values()) {
                input = this.touchControls.processTouch(touch, input);
            }
        }
        return input;
    }
}
