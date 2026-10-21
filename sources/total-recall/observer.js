/* Instrument public gameplay boundaries without changing upstream game rules. */
"use strict";

let captureTick = -1;
let captureEvents = [];

function captureActor(actor) {
    return { ...actor.pos, action: actor.action, shape: actor.shape, gold: actor.hasGold ?? 0 };
}

function captureState() {
    const digging = holeObj.action === ACT_DIGGING;
    const digTarget = digging ? {
        x: holeObj.pos.x, y: holeObj.pos.y + 1,
        active: map[holeObj.pos.x][holeObj.pos.y + 1].act,
        above: map[holeObj.pos.x][holeObj.pos.y].act,
    } : null;
    return {
        state: gameState, player: captureActor(runner), guards: guard.map(captureActor),
        gold: goldCount, digging,
        digFrame: digging ? holeObj.curFrameIdx : null, command: keyAction,
        scheduler: { offset: moveOffset, id: moveId },
        digTarget, holes: fillHoleObj.map(value => ({ ...value.pos })),
    };
}

// Wrappers report both sides of mutations and retain the upstream return value.
function captureWrap(name, event) {
    const original = window[name];
    window[name] = function (...args) {
        const before = captureState();
        const result = original.apply(this, args);
        captureEvents.push({
            tick: captureTick, event, args: args.map(value => typeof value === "object" ? null : value),
            result: name === "getDemoBornPos" || typeof result === "number" ? result : null,
            before, after: captureState(),
        });
        return result;
    };
}

async function captureDemo() {
    try {
        createjs.Ticker.removeAllEventListeners("tick");
        const level = Number(new URLSearchParams(location.search).get("level"));
        const demo = demoData1.find(value => value.level === level);
        if (!demo) throw new Error(`No bundled demo for level ${level}`);
        if (demo.ai < 3) throw new Error("AI versions before 3 require animation-clock observation");
        function reset() {
            playMode = PLAY_DEMO;
            curLevel = level;
            curAiVersion = demo.ai;
            godMode = demo.godMode ?? 0;
            recordMode = RECORD_NONE;
            demoRecord = demo.action;
            demoGoldDrop = demo.goldDrop;
            demoBornPos = demo.bornPos;
            curScore = 0;
            showLevel(classicData[level - 1]);
            initPlayDemo();
        }

        // Every observed tick must match a fresh uninstrumented execution.
        reset();
        const baselineInitial = JSON.stringify(captureState());
        const baseline = [];
        for (let tick = 0; tick <= demo.time + 2; tick++) {
            playGame(1 / demoSpeed);
            baseline.push(JSON.stringify(captureState()));
            if (gameState === GAME_FINISH || gameState === GAME_RUNNER_DEAD) break;
        }
        reset();
        if (JSON.stringify(captureState()) !== baselineInitial) {
            throw new Error("Fresh executions disagree at initialization");
        }

        for (const [name, event] of [
            ["pressKey", "key"], ["runnerMoveStep", "player-step"],
            ["guardMoveStep", "guard-step"], ["digHole", "dig-start"],
            ["digComplete", "dig-complete"], ["getDemoGold", "gold-delay"],
            ["getDemoBornPos", "resurrection"], ["setRunnerDead", "death"],
            ["isDigging", "dig-check"], ["stopDigging", "dig-cancel"],
        ]) captureWrap(name, event);

        const initial = captureState();
        const ticks = [];
        for (captureTick = 0; captureTick <= demo.time + 2; captureTick++) {
            captureEvents = [];
            const before = captureState();
            playGame(1 / demoSpeed);
            if (JSON.stringify(captureState()) !== baseline[captureTick]) {
                throw new Error(`Instrumentation changed gameplay at tick ${captureTick}`);
            }
            ticks.push({ tick: captureTick, before, after: captureState(), events: captureEvents });
            if (gameState === GAME_FINISH || gameState === GAME_RUNNER_DEAD) break;
        }
        mainStage.update();
        await fetch("/capture", { method: "POST", body: JSON.stringify({
            version: 1, level, ai: demo.ai, tile: [tileW, tileH], step: [xMove, yMove],
            observationalEquivalence: true,
            demo, board: classicData[level - 1], initial, ticks, final: captureState(),
        }) });
    } catch (error) {
        await fetch("/capture", { method: "POST", body: JSON.stringify({ error: String(error.message ?? error) }) });
    }
}

// Normal preloading builds real sprites and maps. Only menu entry and online
// demo discovery are replaced; gameplay runs through the upstream playGame path.
function captureInit() {
    window.getDemoData = function () { demoDataLoading = 0; };
    window.waitIdleDemo = captureDemo;
    window.addEventListener("error", event => {
        fetch("/capture", { method: "POST", body: JSON.stringify({ error: event.message }) });
    });
    init();
}
