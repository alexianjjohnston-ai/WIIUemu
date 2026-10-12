// React bindings for the input engine: pointer handling with capture, per-pointer sources.

import { useCallback, useMemo, useSyncExternalStore, type PointerEvent as ReactPointerEvent } from 'react';
import type { InputEngine } from './engine';
import { dpadDirections, type DpadDirection } from './dpad';
import { hapticTap } from '../lib/haptics';

type El = HTMLElement;

const sourceOf = (e: ReactPointerEvent) => `p:${e.pointerId}`;

function capture(e: ReactPointerEvent<El>): void {
  try {
    e.currentTarget.setPointerCapture(e.pointerId);
  } catch {
    // Pointer already gone (or synthetic): the up/cancel handlers still release it.
  }
}

/** Ignore secondary mouse buttons; touch and pen always count. */
function isPrimaryPress(e: ReactPointerEvent): boolean {
  return e.pointerType !== 'mouse' || e.button === 0;
}

/** Is `button` currently pressed? Re-renders only when that answer changes. */
export function usePressed<B extends string>(engine: InputEngine<B>, button: B): boolean {
  const get = useCallback(() => engine.isPressed(button), [engine, button]);
  return useSyncExternalStore(engine.subscribe, get, get);
}

export interface PointerHandlers {
  onPointerDown: (e: ReactPointerEvent<El>) => void;
  onPointerMove?: (e: ReactPointerEvent<El>) => void;
  onPointerUp: (e: ReactPointerEvent<El>) => void;
  onPointerCancel: (e: ReactPointerEvent<El>) => void;
  onLostPointerCapture: (e: ReactPointerEvent<El>) => void;
}

function releaseHandlers(engine: InputEngine) {
  const release = (e: ReactPointerEvent<El>) => {
    engine.releaseSource(sourceOf(e));
  };
  return {
    onPointerUp: release,
    onPointerCancel: release,
    onLostPointerCapture: release,
  };
}

/** Handlers for a single button: each pointer holds it from down until up/cancel/lost capture. */
export function useButtonPointer<B extends string>(engine: InputEngine<B>, button: B): PointerHandlers {
  return useMemo(
    () => ({
      onPointerDown: (e: ReactPointerEvent<El>) => {
        if (!isPrimaryPress(e)) return;
        e.preventDefault();
        capture(e);
        if (engine.press(sourceOf(e), button)) hapticTap();
      },
      ...releaseHandlers(engine),
    }),
    [engine, button],
  );
}

/** Handlers for the D-pad: the direction follows the finger while it stays down (8-way). */
export function useDpadPointer<B extends string>(engine: InputEngine<B | DpadDirection>): PointerHandlers {
  return useMemo(() => {
    const update = (e: ReactPointerEvent<El>) => {
      const rect = e.currentTarget.getBoundingClientRect();
      const dirs: DpadDirection[] = dpadDirections(
        e.clientX - (rect.left + rect.width / 2),
        e.clientY - (rect.top + rect.height / 2),
        Math.min(rect.width, rect.height) / 2,
      );
      const before = engine.getButtons();
      if (engine.setSource(sourceOf(e), dirs) && dirs.some((d) => !before.includes(d))) hapticTap();
    };
    const active = new Set<number>();
    const release = releaseHandlers(engine);
    const end = (e: ReactPointerEvent<El>) => {
      active.delete(e.pointerId);
      release.onPointerUp(e);
    };
    return {
      onPointerDown: (e: ReactPointerEvent<El>) => {
        if (!isPrimaryPress(e)) return;
        e.preventDefault();
        capture(e);
        active.add(e.pointerId);
        update(e);
      },
      onPointerMove: (e: ReactPointerEvent<El>) => {
        if (active.has(e.pointerId)) update(e);
      },
      onPointerUp: end,
      onPointerCancel: end,
      onLostPointerCapture: end,
    };
  }, [engine]);
}
