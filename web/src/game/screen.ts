// A whole game screen: the game view with its inspector and charts, for a
// recorded game or one being played.

import { Charts } from "./charts.ts";
import type { GameData } from "./data.ts";
import { Heatmap } from "./heatmap.ts";
import { Inspector } from "./inspector.ts";
import { GameView } from "./view.ts";

/** What the viewer chose to see, which a live page carries from one game's
 * screen to the next (the preview, then each game). */
export interface Settings {
  /** Whose marks are drawn: the marks menu's value. */
  marks: string;
  /** The heatmap menu's value. */
  heatmap: string;
  charts: boolean;
  speed: number;
}

export interface ScreenOptions {
  turn?: number | null;
  /** The shown turn settled (for the address). */
  onTurn?(turn: number): void;
  /** Choices from an earlier screen; those this game does not offer are left out. */
  settings?: Settings | undefined;
}

export class GameScreen {
  readonly view: GameView;
  readonly inspector: Inspector;
  readonly charts: Charts;
  readonly heatmap: Heatmap;

  constructor(data: GameData, options: ScreenOptions = {}) {
    let view: GameView | null = null;
    this.inspector = new Inspector(data, () => view?.draw());
    this.charts = new Charts(data, (turn) => view?.goTo(turn));
    this.heatmap = new Heatmap(data, () => view?.draw());
    const inspector = this.inspector;
    const charts = this.charts;
    const heatmap = this.heatmap;
    view = new GameView(data, {
      turn: options.turn ?? null,
      onTurn: (turn) => options.onTurn?.(turn),
      onShow(turn) {
        inspector.show(turn);
        charts.mark(turn);
      },
      overlays: () => inspector.overlays(),
      heat: (turn) => heatmap.layer(turn),
    });
    this.view = view;
    view.side.append(this.inspector.element);
    view.boardColumn.append(this.heatmap.element);
    view.element.append(this.charts.element);
    if (options.settings) {
      this.inspector.marks = options.settings.marks;
      this.heatmap.selected = options.settings.heatmap;
      this.charts.open = options.settings.charts;
      view.playbackSpeed = options.settings.speed;
      view.draw();
    }
  }

  get element(): HTMLElement {
    return this.view.element;
  }

  get settings(): Settings {
    return {
      marks: this.inspector.marks,
      heatmap: this.heatmap.selected,
      charts: this.charts.open,
      speed: this.view.playbackSpeed,
    };
  }

  /** The game gained turns or ended. */
  update(): void {
    this.view.update();
    this.charts.update();
  }

  dispose(): void {
    this.view.dispose();
    this.charts.dispose();
  }
}
