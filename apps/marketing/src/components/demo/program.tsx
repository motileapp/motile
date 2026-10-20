import {
  HourglassIcon,
  PartyPopperIcon,
  RotateCcwIcon,
  type LucideIcon,
} from "lucide-react"
import {
  useEffect,
  useRef,
  useState,
  type KeyboardEvent,
  type PointerEvent,
  type ReactNode,
} from "react"
import { cn } from "@/lib/utils"

type Inputs = { initial: number; monthly: number; years: number; rate: number }

type Year = { year: string; Contributions: number; Balance: number }

const START: Inputs = { initial: 10000, monthly: 500, years: 30, rate: 5 }

const SERIES = ["Contributions", "Balance"] as const

const COLORS = ["var(--chart-1)", "var(--chart-2)"]

function project({ initial, monthly, years, rate }: Inputs): Year[] {
  const monthlyRate = rate / 1200
  return Array.from({ length: years + 1 }, (_, y) => {
    const Contributions = initial + monthly * 12 * y
    const growth = (1 + monthlyRate) ** (12 * y)
    const Balance =
      rate === 0
        ? Contributions
        : initial * growth + (monthly * (growth - 1)) / monthlyRate
    return { year: `Year ${y}`, Contributions, Balance }
  })
}

const dollars = new Intl.NumberFormat("en-US", {
  style: "currency",
  currency: "USD",
  maximumFractionDigits: 0,
})

/**
 * `fixtures/openui/savings.oui` as the Mac app draws it: core's resolved tree written out by hand,
 * and what the program computes from its four sliders.
 */
export function SavingsProgram() {
  const [inputs, setInputs] = useState(START)
  const [column, ref] = useWidth(768)
  const rows = project(inputs)
  const final = rows[rows.length - 1]
  const million = rows.find((row) => row.Balance >= 1000000)
  const inner = column - 40
  const set = (key: keyof Inputs) => (value: number) =>
    setInputs((all) => ({ ...all, [key]: value }))

  return (
    <div ref={ref}>
      <section className="flex flex-col gap-4 rounded-lg border border-border-card bg-card p-[19px] text-foreground">
        <div className="flex items-start gap-3">
          <div className="flex min-w-0 flex-col gap-1">
            <h3 className="text-[17px] leading-6 font-semibold tabular-nums">
              Retirement savings
            </h3>
            <p className="text-[13px] leading-5 text-muted-foreground">
              Turn today's contributions into a picture of tomorrow.
            </p>
          </div>
          <span className="flex-1" />
          <GhostButton icon={RotateCcwIcon} onClick={() => setInputs(START)}>
            Reset
          </GhostButton>
        </div>
        <Grid columns={3} least={150} gap={12} width={inner}>
          <Figure
            caption={`Balance in ${inputs.years} ${inputs.years === 1 ? "year" : "years"}`}
            value={dollars.format(final.Balance)}
          />
          <Figure
            caption="You put in"
            value={dollars.format(final.Contributions)}
          />
          <Figure
            caption="Growth"
            value={dollars.format(final.Balance - final.Contributions)}
            success
          />
        </Grid>
        <AreaChart rows={rows} width={inner} />
        <div className="flex items-center gap-2 text-[13px] leading-5">
          {million ? (
            <PartyPopperIcon className="size-3.5 shrink-0 text-success" />
          ) : (
            <HourglassIcon className="size-3.5 shrink-0 text-muted-foreground" />
          )}
          <p className="min-w-0">
            {million ? (
              <>
                You pass <strong className="font-semibold">$1 million</strong>{" "}
                in {million.year},{" "}
                {Math.round(
                  ((million.Balance - million.Contributions) /
                    million.Balance) *
                    100
                )}
                % of it from growth.
              </>
            ) : (
              "At this pace the balance stays under $1 million."
            )}
          </p>
        </div>
        <Grid columns={2} least={240} gap={24} width={inner}>
          <Slider
            label="Starting balance"
            value={inputs.initial}
            min={0}
            max={200000}
            step={1000}
            readout={(value) => `$${value.toLocaleString("en-US")}`}
            onChange={set("initial")}
          />
          <Slider
            label="Monthly contribution"
            value={inputs.monthly}
            min={0}
            max={5000}
            step={50}
            readout={(value) => `$${value.toLocaleString("en-US")}`}
            onChange={set("monthly")}
          />
          <Slider
            label="Years"
            value={inputs.years}
            min={1}
            max={50}
            step={1}
            readout={(value) => `${value} yrs`}
            onChange={set("years")}
          />
          <Slider
            label="Annual return"
            value={inputs.rate}
            min={0}
            max={12}
            step={0.5}
            readout={(value) => `${value}%`}
            onChange={set("rate")}
          />
        </Grid>
        <p className="text-[12px] leading-[18px] text-muted-foreground">
          Assumes monthly compounding and contributions at the end of each
          month. Returns are not guaranteed; this is an illustration, not
          advice.
        </p>
      </section>
    </div>
  )
}

/** The width of what `ref` is set on, which the program is laid out at. */
function useWidth(initial: number) {
  const ref = useRef<HTMLDivElement>(null)
  const [width, setWidth] = useState(initial)
  useEffect(() => {
    const element = ref.current
    if (!element) return
    const observer = new ResizeObserver(() => setWidth(element.offsetWidth))
    observer.observe(element)
    return () => observer.disconnect()
  }, [])
  return [width, ref] as const
}

/** Grid's columns: at most `columns`, each at least `least` wide, one when even one doesn't fit. */
function Grid({
  columns,
  least,
  gap,
  width,
  children,
}: {
  columns: number
  least: number
  gap: number
  width: number
  children: ReactNode
}) {
  const share = (width - (columns - 1) * gap) / columns
  const column = Math.min(width, Math.max(least, share))
  const count = Math.max(
    1,
    Math.min(columns, Math.floor((width + gap) / (column + gap)))
  )
  return (
    <div
      className="grid"
      style={{ gridTemplateColumns: `repeat(${count}, minmax(0, 1fr))`, gap }}
    >
      {children}
    </div>
  )
}

/** A soft Card nested in the program's: a Caption and a Title xl. */
function Figure({
  caption,
  value,
  success = false,
}: {
  caption: string
  value: string
  success?: boolean
}) {
  return (
    <div className="flex flex-col gap-1 rounded-xs bg-card-accent p-3">
      <p className="text-[12px] leading-[18px] text-muted-foreground">
        {caption}
      </p>
      <p
        className={cn(
          "text-[16px] leading-[22px] font-semibold tabular-nums",
          success && "text-success"
        )}
      >
        {value}
      </p>
    </div>
  )
}

function GhostButton({
  icon: Icon,
  onClick,
  children,
}: {
  icon: LucideIcon
  onClick: () => void
  children: string
}) {
  return (
    <button
      type="button"
      onClick={onClick}
      className="flex h-6 shrink-0 items-center gap-[5px] rounded-sm px-2 text-[11.5px] font-medium text-muted-foreground hover:bg-card-accent hover:text-foreground"
    >
      <Icon className="size-4" />
      {children}
    </button>
  )
}

const THUMB = 18
const REACH = 12

/** The kits' Slider: the label and its readout, the round track and its thumb, which a drag or the arrow keys move. */
function Slider({
  label,
  value,
  min,
  max,
  step,
  readout,
  onChange,
}: {
  label: string
  value: number
  min: number
  max: number
  step: number
  readout: (value: number) => string
  onChange: (value: number) => void
}) {
  const [dragging, setDragging] = useState(false)
  const at = (value - min) / (max - min)
  const snapped = (raw: number) =>
    Math.min(max, Math.max(min, min + Math.round((raw - min) / step) * step))

  const move = (event: PointerEvent<HTMLDivElement>) => {
    const box = event.currentTarget.getBoundingClientRect()
    const width = event.currentTarget.offsetWidth
    const x = ((event.clientX - box.left) * width) / box.width
    const fraction = Math.min(1, Math.max(0, (x - REACH) / (width - 2 * REACH)))
    onChange(snapped(min + fraction * (max - min)))
  }
  const key = (event: KeyboardEvent) => {
    const steps = { ArrowRight: 1, ArrowUp: 1, ArrowLeft: -1, ArrowDown: -1 }[
      event.key
    ]
    if (!steps) return
    event.preventDefault()
    onChange(snapped(value + steps * step))
  }

  return (
    <div>
      <div className="flex items-baseline justify-between gap-3">
        <span className="text-[13px] leading-5">{label}</span>
        <span className="shrink-0 text-[12px] leading-[18px] font-medium whitespace-nowrap tabular-nums">
          {readout(value)}
        </span>
      </div>
      <div
        role="slider"
        tabIndex={0}
        aria-label={label}
        aria-valuemin={min}
        aria-valuemax={max}
        aria-valuenow={value}
        aria-valuetext={readout(value)}
        onKeyDown={key}
        onPointerDown={(event) => {
          event.currentTarget.setPointerCapture(event.pointerId)
          setDragging(true)
          move(event)
        }}
        onPointerMove={(event) => dragging && move(event)}
        onPointerUp={() => setDragging(false)}
        className="group relative flex h-9 touch-none items-center rounded-full outline-none"
      >
        <div className="relative h-6 w-full rounded-full bg-card-accent group-focus-visible:ring-2 group-focus-visible:ring-ring">
          <span
            className="absolute top-[3px] rounded-full bg-foreground"
            style={{
              width: THUMB,
              height: THUMB,
              left: `calc(${REACH - THUMB / 2}px + (100% - ${2 * REACH}px) * ${at})`,
            }}
          />
        </div>
      </div>
    </div>
  )
}

const HEIGHT = 240
const TOP = 10
const BOTTOM = 26
const RIGHT = 4
const TIP_INSET = 70

/** A step of 1, 2, 2.5, 5 or 10 times a power of ten, at least `span`. */
function niceStep(span: number) {
  if (!(span > 0)) return 1
  const power = 10 ** Math.floor(Math.log10(span))
  const n = span / power
  const factor = n <= 1 ? 1 : n <= 2 ? 2 : n <= 2.5 ? 2.5 : n <= 5 ? 5 : 10
  return factor * power
}

/** A currency as a chart writes it: compact on the axis ("$500K"), in full in a tooltip. */
function money(value: number, compact: boolean) {
  const big = Math.abs(value) >= (compact ? 1000 : 1e6)
  return new Intl.NumberFormat("en-US", {
    style: "currency",
    currency: "USD",
    minimumFractionDigits: 0,
    maximumFractionDigits: compact && big ? 1 : 2,
    notation: big ? "compact" : "standard",
  }).format(value)
}

const labelWidth = (label: string, size: number) => label.length * size * 0.7

const tenth = (value: number) => Number(value.toFixed(1))

/** An area chart of the rows' two series, laid out as core's `canvas/chart.rs` does. */
function AreaChart({ rows, width }: { rows: Year[]; width: number }) {
  const [lit, setLit] = useState<number | null>(null)
  const max = Math.max(0, ...rows.flatMap((row) => SERIES.map((s) => row[s])))
  const step = niceStep(max / 5)
  const hi = Math.max(step, Math.ceil(max / step) * step)
  const ticks = Array.from(
    { length: Math.round(hi / step) + 1 },
    (_, k) => k * step
  )
  const left =
    Math.max(...ticks.map((tick) => labelWidth(money(tick, true), 11))) + 12
  const w = Math.max(10, width - left - RIGHT)
  const h = HEIGHT - TOP - BOTTOM
  const band = w / rows.length
  const xAt = (i: number) => left + band * i + band / 2
  const y = (v: number) => TOP + h - (v / hi) * h
  const longest = Math.max(1, ...rows.map((row) => row.year.length))
  const every = Math.max(1, Math.ceil((longest * 6.6 + 10) / band))

  const point = (event: PointerEvent<SVGSVGElement>) => {
    const box = event.currentTarget.getBoundingClientRect()
    const x = ((event.clientX - box.left) * width) / box.width
    const i = Math.floor((x - left) / band)
    setLit(i >= 0 && i < rows.length ? i : null)
  }

  return (
    <div>
      <div className="relative">
        <svg
          width={width}
          height={HEIGHT}
          className="block overflow-visible"
          role="img"
          aria-label="area chart of Contributions, Balance by year"
          onPointerMove={point}
          onPointerLeave={() => setLit(null)}
        >
          <defs>
            {SERIES.map((series, si) => {
              const top = Math.min(...rows.map((row) => tenth(y(row[series]))))
              return (
                <linearGradient
                  key={series}
                  id={`savings-${series}`}
                  gradientUnits="userSpaceOnUse"
                  x1={0}
                  x2={0}
                  y1={top}
                  y2={tenth(y(0))}
                >
                  <stop offset={0} stopColor={COLORS[si]} stopOpacity={0.24} />
                  <stop offset={1} stopColor={COLORS[si]} stopOpacity={0.02} />
                </linearGradient>
              )
            })}
          </defs>
          {ticks.map((tick) => (
            <g key={tick}>
              <line
                x1={left}
                x2={left + w}
                y1={y(tick)}
                y2={y(tick)}
                stroke="var(--border-card)"
                strokeDasharray={tick === 0 ? undefined : "3 4"}
              />
              <text
                x={left - 10}
                y={y(tick) + 4}
                textAnchor="end"
                className="fill-muted-stronger-foreground text-[11px] tabular-nums"
              >
                {money(tick, true)}
              </text>
            </g>
          ))}
          {rows.map((row, i) => {
            if (i % every !== 0) return null
            const half = labelWidth(row.year, 12) / 2
            const [x, anchor]: [number, "start" | "middle" | "end"] =
              xAt(i) - half < 0
                ? [0, "start"]
                : xAt(i) + half > width
                  ? [width, "end"]
                  : [xAt(i), "middle"]
            return (
              <text
                key={row.year}
                x={x}
                y={HEIGHT - 6}
                textAnchor={anchor}
                className="fill-muted-foreground text-[12px]"
              >
                {row.year}
              </text>
            )
          })}
          {SERIES.map((series, si) => {
            const line = rows.map(
              (row, i) => `${tenth(xAt(i))},${tenth(y(row[series]))}`
            )
            const base = `${tenth(xAt(rows.length - 1))},${tenth(y(0))} ${tenth(xAt(0))},${tenth(y(0))}`
            return (
              <g key={series}>
                {rows.length > 1 && (
                  <polygon
                    points={`${line.join(" ")} ${base}`}
                    fill={`url(#savings-${series})`}
                  />
                )}
                <polyline
                  points={line.join(" ")}
                  fill="none"
                  stroke={COLORS[si]}
                  strokeWidth={2.2}
                  strokeLinecap="round"
                  strokeLinejoin="round"
                />
              </g>
            )
          })}
          {lit !== null && (
            <g>
              {SERIES.map((series, si) => (
                <circle
                  key={series}
                  cx={xAt(lit)}
                  cy={y(rows[lit][series])}
                  r={4.5}
                  fill={COLORS[si]}
                  stroke="var(--card)"
                  strokeWidth={1.5}
                />
              ))}
              <line
                x1={xAt(lit)}
                x2={xAt(lit)}
                y1={TOP}
                y2={TOP + h}
                stroke="var(--border-accent)"
                strokeDasharray="3 3"
              />
            </g>
          )}
        </svg>
        {lit !== null && (
          <ChartTip
            x={Math.min(Math.max(xAt(lit), TIP_INSET), width - TIP_INSET)}
            row={rows[lit]}
          />
        )}
      </div>
      <div className="mt-2.5 flex flex-wrap gap-x-4 gap-y-1.5">
        {SERIES.map((series, si) => (
          <span key={series} className="flex items-center gap-[7px]">
            <span
              className="size-2 rounded-full"
              style={{ background: COLORS[si] }}
            />
            <span className="text-[12px] leading-[18px] text-muted-foreground">
              {series}
            </span>
          </span>
        ))}
      </div>
    </div>
  )
}

function ChartTip({ x, row }: { x: number; row: Year }) {
  return (
    <div
      className="pointer-events-none absolute flex min-w-[110px] -translate-x-1/2 flex-col gap-[3px] rounded-lg border border-border-popover bg-popover px-3 py-2.5 text-[12px] leading-[18px] shadow-md shadow-shadow/(--opacity-shadow)"
      style={{ left: x, top: TOP }}
    >
      <span className="font-semibold">{row.year}</span>
      {SERIES.map((series, si) => (
        <span key={series} className="flex items-center gap-1.5">
          <span
            className="size-2 shrink-0 rounded-full"
            style={{ background: COLORS[si] }}
          />
          <span className="mr-2 text-muted-foreground">{series}</span>
          <span className="ml-auto font-medium whitespace-nowrap tabular-nums">
            {money(row[series], false)}
          </span>
        </span>
      ))}
    </div>
  )
}
