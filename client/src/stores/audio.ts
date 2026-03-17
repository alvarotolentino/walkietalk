import { createSignal } from "solid-js";

const [isTransmitting, setIsTransmitting] = createSignal(false);
const [isReceiving, setIsReceiving] = createSignal(false);
const [sendLevel, setSendLevel] = createSignal(0);
const [recvLevel, setRecvLevel] = createSignal(0);
const [floorTimeRemaining, setFloorTimeRemaining] = createSignal(60);

export {
  isTransmitting,
  isReceiving,
  sendLevel,
  recvLevel,
  floorTimeRemaining,
};

let countdownTimer: ReturnType<typeof setInterval> | null = null;

export function startTransmitting() {
  setIsTransmitting(true);
  setFloorTimeRemaining(60);
  countdownTimer = setInterval(() => {
    setFloorTimeRemaining((v) => {
      if (v <= 0) {
        stopTransmitting();
        return 0;
      }
      return v - 1;
    });
  }, 1000);
}

export function stopTransmitting() {
  setIsTransmitting(false);
  setSendLevel(0);
  if (countdownTimer) {
    clearInterval(countdownTimer);
    countdownTimer = null;
  }
}

export function startReceiving() {
  setIsReceiving(true);
}

export function stopReceiving() {
  setIsReceiving(false);
  setRecvLevel(0);
}

export function updateSendLevel(level: number) {
  setSendLevel(level);
}

export function updateRecvLevel(level: number) {
  setRecvLevel(level);
}

export function resetAudioState() {
  stopTransmitting();
  stopReceiving();
  setSendLevel(0);
  setRecvLevel(0);
  setFloorTimeRemaining(60);
}
