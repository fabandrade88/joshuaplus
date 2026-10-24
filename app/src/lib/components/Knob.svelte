<script lang="ts">
  // Rotary knob: drag vertically (Shift = fine), arrow keys, double-click resets.
  let {
    value,
    min,
    max,
    reset,
    label,
    display,
    size = 38,
    color = "var(--accent)",
    bipolar = false,
    onchange,
  }: {
    value: number;
    min: number;
    max: number;
    reset: number;
    label: string;
    display: string;
    size?: number;
    color?: string;
    bipolar?: boolean;
    onchange: (v: number) => void;
  } = $props();

  const norm = $derived((value - min) / (max - min));
  const angle = $derived(-135 + norm * 270);
  let drag: { y: number; v: number } | null = null;

  const clamp = (v: number) => Math.min(max, Math.max(min, v));

  function down(e: PointerEvent): void {
    (e.currentTarget as HTMLElement).setPointerCapture(e.pointerId);
    drag = { y: e.clientY, v: value };
  }
  function move(e: PointerEvent): void {
    if (!drag) return;
    const range = max - min;
    const speed = e.shiftKey ? 600 : 150;
    onchange(clamp(drag.v + ((drag.y - e.clientY) / speed) * range));
  }
  function key(e: KeyboardEvent): void {
    const step = (max - min) / (e.shiftKey ? 200 : 40);
    if (e.key === "ArrowUp" || e.key === "ArrowRight") onchange(clamp(value + step));
    else if (e.key === "ArrowDown" || e.key === "ArrowLeft") onchange(clamp(value - step));
    else return;
    e.preventDefault();
  }

  // Arc path for the value ring.
  function arc(from: number, to: number): string {
    const r = 15;
    const p = (deg: number) => {
      const a = ((deg - 90) * Math.PI) / 180;
      return `${18 + r * Math.cos(a)} ${18 + r * Math.sin(a)}`;
    };
    const large = Math.abs(to - from) > 180 ? 1 : 0;
    const sweep = to > from ? 1 : 0;
    return `M ${p(from)} A ${r} ${r} 0 ${large} ${sweep} ${p(to)}`;
  }
</script>

<div class="knob" title="{label} · {display}">
  <div
    class="dial"
    role="slider"
    tabindex="0"
    aria-label={label}
    aria-valuemin={min}
    aria-valuemax={max}
    aria-valuenow={value}
    aria-valuetext={display}
    style={`width:${size}px;height:${size}px;--c:${color}`}
    onpointerdown={down}
    onpointermove={move}
    onpointerup={() => (drag = null)}
    onpointercancel={() => (drag = null)}
    ondblclick={() => onchange(reset)}
    onkeydown={key}
  >
    <svg viewBox="0 0 36 36" aria-hidden="true">
      <path d={arc(-135, 135)} class="track" />
      {#if bipolar}
        {#if Math.abs(angle) > 1}<path d={arc(0, angle)} class="value" />{/if}
      {:else if norm > 0.004}
        <path d={arc(-135, angle)} class="value" />
      {/if}
      <g style={`transform: rotate(${angle}deg); transform-origin: 18px 18px`}>
        <circle cx="18" cy="18" r="10.5" class="cap" />
        <line x1="18" y1="9.5" x2="18" y2="14" class="pointer" />
      </g>
    </svg>
  </div>
</div>

<style>
  .knob {
    display: grid;
    justify-items: center;
  }
  .dial {
    cursor: ns-resize;
    touch-action: none;
    border-radius: 50%;
  }
  svg {
    width: 100%;
    height: 100%;
    overflow: visible;
  }
  .track {
    fill: none;
    stroke: var(--line-strong);
    stroke-width: 2.5;
    stroke-linecap: round;
  }
  .value {
    fill: none;
    stroke: var(--c);
    stroke-width: 2.5;
    stroke-linecap: round;
  }
  .cap {
    fill: #262c3d;
    stroke: #39415a;
    stroke-width: 1;
  }
  .pointer {
    stroke: var(--text);
    stroke-width: 2;
    stroke-linecap: round;
  }
</style>
