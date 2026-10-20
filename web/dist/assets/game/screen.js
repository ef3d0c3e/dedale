// A whole game screen: the game view with its inspector and charts, for a
// recorded game or one being played.
import { Charts } from "./charts.js";
import { Heatmap } from "./heatmap.js";
import { Inspector } from "./inspector.js";
import { GameView } from "./view.js";
export class GameScreen {
    view;
    inspector;
    charts;
    heatmap;
    constructor(data, options = {}) {
        let view = null;
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
    get element() {
        return this.view.element;
    }
    get settings() {
        return {
            marks: this.inspector.marks,
            heatmap: this.heatmap.selected,
            charts: this.charts.open,
            speed: this.view.playbackSpeed,
        };
    }
    /** The game gained turns or ended. */
    update() {
        this.view.update();
        this.charts.update();
    }
    dispose() {
        this.view.dispose();
        this.charts.dispose();
    }
}
