import { InputBits } from "../message";
import { TouchControls, Touch, TouchArea } from "./touch_controls";

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

export interface Rect {
    bottom: number;
    height: number;
    left: number;
    right: number;
    top: number;
    width: number;
    x: number;
    y: number;
}

export namespace Rect {
    export function fromParts(
        bottom: number,
        left: number,
        width: number,
        height: number
    ) {
        return {
            bottom,
            height,
            left,
            right: left + width,
            top: bottom - height,
            width,
            x: left,
            y: bottom - height,
        };
    }

    export function contains(rect: Rect, x: number, y: number): boolean {
        return (
            x >= rect.left && x < rect.right && y >= rect.top && y < rect.bottom
        );
    }
}

export interface InputChanges {
    pressed: number;
    released: number;
    touchPos: [number, number] | null | undefined;
}

// TODO: Allow configuring this
const limitTouchesToBottomScreenRect = false;

export class Input {
    public controls: HTMLElement;
    private pauseButtonContainer: HTMLElement;
    private pauseButton: HTMLButtonElement;

    private touch_!: boolean;
    public touchControls?: TouchControls;
    private touches: Map<number, Touch> = new Map();

    private mouseDownCallback: (e: MouseEvent) => void;
    private mouseMoveCallback: (e: MouseEvent) => void;
    private mouseUpCallback: (e: MouseEvent) => void;

    private touchStartCallback: (e: TouchEvent) => void;
    private touchMoveCallback: (e: TouchEvent) => void;
    private touchEndCallback: (e: TouchEvent) => void;

    private prevInput: number;
    private pressedKeys: number;

    private botScreenRect?: Rect;
    private botScreenTouchX: number | undefined;
    private botScreenTouchY: number | undefined;

    constructor(touch: boolean, private pauseCallback: () => void) {
        this.controls = document.getElementById("controls")!;
        this.pauseButtonContainer = document.getElementById("btn-pause")!;
        this.pauseButton = this.pauseButtonContainer.getElementsByTagName(
            "button"
        )[0] as HTMLButtonElement;

        this.mouseDownCallback = this.mouseDown.bind(this);
        this.mouseMoveCallback = this.mouseMove.bind(this);
        this.mouseUpCallback = this.mouseUp.bind(this);

        this.touchStartCallback = this.touchStart.bind(this);
        this.touchMoveCallback = this.touchMove.bind(this);
        this.touchEndCallback = this.touchEnd.bind(this);

        this.prevInput = 0;
        this.pressedKeys = 0;

        document.body.addEventListener("keydown", (e) => {
            this.pressedKeys |= keyToInputBit[e.key] ?? 0;
        });
        document.body.addEventListener("keyup", (e) => {
            this.pressedKeys &= ~(keyToInputBit[e.key] ?? 0);
        });

        this.controls.addEventListener("mousedown", this.mouseDownCallback);
        this.controls.addEventListener("mousemove", this.mouseMoveCallback);
        window.addEventListener("mouseup", this.mouseUpCallback);

        this.controls.addEventListener("touchstart", this.touchStartCallback);
        this.controls.addEventListener("touchmove", this.touchMoveCallback);
        window.addEventListener("touchend", this.touchEndCallback);
        window.addEventListener("touchcancel", this.touchEndCallback);

        this.pauseButton.addEventListener("click", pauseCallback);

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
            this.touchControls = new TouchControls(this.controls);
            this.touchControls.layoutData = this.touchControls.defaultLayout; // TODO: Remove
        } else {
            delete this.touchControls;
        }
    }

    setTouch(id: number, x: number, y: number) {
        this.touches.set(id, {
            area: this.touchControls?.containTouch(x, y)
                ? TouchArea.Controls
                : !limitTouchesToBottomScreenRect ||
                  (this.botScreenRect &&
                      Rect.contains(this.botScreenRect, x, y))
                ? TouchArea.BottomScreen
                : TouchArea.None,
            startX: x,
            startY: y,
            x,
            y,
        });
    }

    mouseDown(e: MouseEvent) {
        this.setTouch(-1, e.clientX, e.clientY);
    }

    touchStart(e: TouchEvent) {
        if (!this.pauseButtonContainer.contains(e.target as Node)) {
            e.preventDefault();
        }
        for (let i = 0; i < e.changedTouches.length; ++i) {
            const t = e.changedTouches[i]!;
            this.setTouch(t.identifier, t.clientX, t.clientY);
        }
    }

    mouseMove(e: MouseEvent) {
        const touch = this.touches.get(-1);
        if (touch) {
            touch.x = e.clientX;
            touch.y = e.clientY;
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

    mouseUp(e: MouseEvent) {
        this.touches.delete(-1);
    }

    touchEnd(e: TouchEvent) {
        for (let i = 0; i < e.changedTouches.length; ++i) {
            const t = e.changedTouches[i]!;
            this.touches.delete(t.identifier);
        }
    }

    update(botScreenRect: Rect): InputChanges | null {
        let input = this.pressedKeys;

        const botScreenHalfWidth = botScreenRect.width * 0.5;
        const botScreenHalfHeight = botScreenRect.height * 0.5;
        const botScreenCenterX = botScreenRect.x + botScreenHalfWidth;
        const botScreenCenterY = botScreenRect.y + botScreenHalfHeight;
        let botScreenTouches = 0;
        let botScreenTouchSumX = 0;
        let botScreenTouchSumY = 0;

        for (const touch of this.touches.values()) {
            if (
                touch.area === TouchArea.None &&
                Rect.contains(botScreenRect, touch.x, touch.y)
            ) {
                touch.area = TouchArea.BottomScreen;
            }
            if (touch.area === TouchArea.BottomScreen) {
                const x = touch.x - botScreenCenterX;
                const y = touch.y - botScreenCenterY;
                const scale = Math.min(
                    Math.abs(botScreenHalfWidth / x),
                    Math.abs(botScreenHalfHeight / y),
                    1
                );
                botScreenTouchSumX += botScreenHalfWidth + x * scale;
                botScreenTouchSumY += botScreenHalfHeight + y * scale;
                botScreenTouches++;
            }
        }

        const touchPos: [number, number] | null = botScreenTouches
            ? [
                  Math.floor(
                      Math.min(
                          ((botScreenTouchSumX / botScreenTouches) * 256) /
                              botScreenRect.width,
                          255
                      )
                  ),
                  Math.floor(
                      Math.min(
                          ((botScreenTouchSumY / botScreenTouches) * 192) /
                              botScreenRect.height,
                          191
                      )
                  ),
              ]
            : null;
        const touchPosChanged =
            touchPos?.[0] !== this.botScreenTouchX ||
            touchPos?.[1] !== this.botScreenTouchY;
        this.botScreenRect = Object.assign({}, botScreenRect);
        this.botScreenTouchX = touchPos?.[0];
        this.botScreenTouchY = touchPos?.[1];

        if (this.touchControls) {
            this.touchControls.resetTouches();
            for (const touch of this.touches.values()) {
                if (touch.area === TouchArea.Controls) {
                    input = this.touchControls.processTouch(touch, input);
                }
            }
        }

        const prevInput = this.prevInput;
        this.prevInput = input;

        return input !== prevInput || touchPosChanged
            ? {
                  pressed: input & ~prevInput,
                  released: prevInput & ~input,
                  touchPos: touchPosChanged ? touchPos : undefined,
              }
            : null;
    }
}
