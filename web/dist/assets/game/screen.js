// A whole game screen: the game view with its inspector and charts, for a
// recorded game or one being played.
import { Charts } from "./charts.js";
import { Inspector } from "./inspector.js";
import { GameView } from "./view.js";
export class GameScreen {
    view;
    inspector;
    charts;
    constructor(data, options = {}) {
        let view = null;
        this.inspector = new Inspector(data, () => view?.draw());
        this.charts = new Charts(data, (turn) => view?.goTo(turn));
        const inspector = this.inspector;
        const charts = this.charts;
        view = new GameView(data, {
            turn: options.turn ?? null,
            onTurn: (turn) => options.onTurn?.(turn),
            onShow(turn) {
                inspector.show(turn);
                charts.mark(turn);
            },
            overlay: () => inspector.overlay(),
        });
        this.view = view;
        view.side.append(this.inspector.element);
        view.element.append(this.charts.element);
    }
    get element() {
        return this.view.element;
    }
    /** The game gained turns or ended. */
    update() {
        this.view.update();
        this.charts.update();
    }
    dispose() {
        this.view.dispose();
    }
}
