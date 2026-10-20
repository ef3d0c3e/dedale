// A whole game screen: the game view with its inspector and charts, for a
// recorded game or one being played.

import { Charts } from "./charts.ts";
import type { GameData } from "./data.ts";
import { Heatmap } from "./heatmap.ts";
import { Inspector } from "./inspector.ts";
import { GameView } from "./view.ts";

export interface ScreenOptions {
  turn?: number | null;
  /** The shown turn settled (for the address). */
  onTurn?(turn: number): void;
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
      overlay: () => inspector.overlay(),
      heat: (turn) => heatmap.layer(turn),
    });
    this.view = view;
    view.side.append(this.inspector.element);
    view.boardColumn.append(this.heatmap.element);
    view.element.append(this.charts.element);
  }

  get element(): HTMLElement {
    return this.view.element;
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
