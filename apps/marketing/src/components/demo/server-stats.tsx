import {
  ArrowDownIcon,
  ArrowUpIcon,
  CpuIcon,
  HardDriveIcon,
  MemoryStickIcon,
  type LucideIcon,
} from "lucide-react"
import { useEffect, useState, type ReactNode } from "react"
import type { Server } from "./threads"

/** Five minutes of samples three seconds apart, as the server keeps them. */
const SLOTS = 100
/** The samples the demo goes round, the last of the first window the ones it opens on. */
const LOOP = 300
const STEP_MS = 3000
const CHART_WIDTH = 300 - 2 * 14
const CHART_HEIGHT = 12
const GIB = 1024 ** 3

type Machine = {
  seed: number
  uptime: string
  cores: number
  memoryTotal: number
  memoryShare: number
  diskTotal: number
  diskUsed: number
  busy: number
  latest: { cpu: number; memoryUsed: number; received: number; sent: number }
}

const MACHINES: Record<string, Machine> = {
  studio: {
    seed: 17,
    uptime: "11d 4h",
    cores: 12,
    memoryTotal: 16 * GIB,
    memoryShare: 0.5,
    diskTotal: 1e12,
    diskUsed: 770e9,
    busy: 0.06,
    latest: { cpu: 9.6, memoryUsed: 8.2 * GIB, received: 42_500, sent: 7_200 },
  },
  "build-box": {
    seed: 63,
    uptime: "3d 19h",
    cores: 16,
    memoryTotal: 32 * GIB,
    memoryShare: 0.38,
    diskTotal: 2e12,
    diskUsed: 1.1e12,
    busy: 0.1,
    latest: { cpu: 14.2, memoryUsed: 12.3 * GIB, received: 3_100, sent: 860 },
  },
}

type Sample = {
  cpu: number
  memoryUsed: number
  received: number
  sent: number
}

/** A fixed series for a machine: a calm CPU with a few spikes, steady memory and sparse traffic. */
function series(machine: Machine): Sample[] {
  const random = seeded(machine.seed)
  const traffic = (peak: number) => {
    const roll = random()
    if (roll < 0.6) return 0
    if (roll < 0.97) return Math.round(200 + random() * 2_800)
    return Math.round(peak * (0.4 + random() * 0.6))
  }
  const samples = Array.from({ length: LOOP }, () => ({
    cpu: random() < machine.busy ? 25 + random() * 60 : 1 + random() ** 2 * 24,
    memoryUsed:
      machine.memoryTotal * (machine.memoryShare + (random() - 0.5) * 0.04),
    received: traffic(60_000),
    sent: traffic(24_000),
  }))
  samples[SLOTS - 1] = machine.latest
  return samples
}

/** Mulberry32: the same numbers for the same seed. */
function seeded(seed: number) {
  let state = seed
  return () => {
    state = (state + 0x6d2b79f5) | 0
    let t = Math.imul(state ^ (state >>> 15), 1 | state)
    t = (t + Math.imul(t ^ (t >>> 7), 61 | t)) ^ t
    return ((t ^ (t >>> 14)) >>> 0) / 4294967296
  }
}

type Tone = "success" | "pending" | "warning"

const toneOf = (share: number, calm = false): Tone =>
  calm || share < 0.6 ? "success" : share < 0.8 ? "pending" : "warning"

const BAR_FILL: Record<Tone, string> = {
  success: "fill-success",
  pending: "fill-pending",
  warning: "fill-warning",
}

const PILL_FILL: Record<Tone, string> = {
  success: "bg-success",
  pending: "bg-pending",
  warning: "bg-warning",
}

const percent = (value: number) =>
  value > 0 && value < 10 ? `${value.toFixed(1)}%` : `${Math.round(value)}%`

/** As the Mac counts them: memory in 1024s, disks and traffic in 1000s. */
function bytes(amount: number, step = 1000) {
  const units = ["KB", "MB", "GB", "TB", "PB"]
  if (amount < step) return `${Math.round(amount)} B`
  let value = amount / step
  let unit = 0
  while (value >= step - 0.5 && unit < units.length - 1) {
    value /= step
    unit += 1
  }
  return `${value >= 100 ? Math.round(value) : value.toFixed(1)} ${units[unit]}`
}

const rate = (amount: number) => `${bytes(amount)}/s`

/** What a server's machine does, as its dropdown in the sidebar shows it, following it while it is open. */
export function ServerStatsCard({ server }: { server: Server }) {
  const machine = MACHINES[server.name] ?? MACHINES.studio
  const [samples] = useState(() => series(machine))
  const [tick, setTick] = useState(0)

  useEffect(() => {
    const timer = setInterval(() => setTick((tick) => tick + 1), STEP_MS)
    return () => clearInterval(timer)
  }, [])

  const shown = Array.from(
    { length: SLOTS },
    (_, index) => samples[(tick + index) % LOOP]
  )
  const latest = shown[SLOTS - 1]
  const peak = (of: (sample: Sample) => number) =>
    Math.max(1000, ...shown.map(of))
  const receivedPeak = peak((sample) => sample.received)
  const sentPeak = peak((sample) => sample.sent)
  const memoryShare = latest.memoryUsed / machine.memoryTotal
  const diskShare = machine.diskUsed / machine.diskTotal

  return (
    <div className="w-[300px] overflow-hidden rounded-md border border-border-popover bg-popover shadow-md">
      <div className="flex flex-col gap-0.5 pt-2 pr-3 pb-2.5 pl-3.5">
        <p className="text-(length:--type-sm) leading-4 font-medium text-foreground">
          {server.name}
        </p>
        <p className="text-(length:--type-xs) leading-[15px] text-muted-foreground tabular-nums">
          Uptime: {machine.uptime}
        </p>
      </div>
      <div className="h-px bg-border-popover" />
      <div className="flex flex-col gap-5 px-3.5 pt-2.5 pb-3.5">
        <StatsRow
          icon={CpuIcon}
          label="CPU"
          detail={`${machine.cores} cores`}
          value={percent(latest.cpu)}
        >
          <StatsBars shares={shown.map((sample) => sample.cpu / 100)} />
        </StatsRow>
        <StatsRow
          icon={MemoryStickIcon}
          label="RAM"
          detail={bytes(machine.memoryTotal, 1024)}
          used={bytes(latest.memoryUsed, 1024)}
          value={percent(memoryShare * 100)}
        >
          <StatsBars
            shares={shown.map(
              (sample) => sample.memoryUsed / machine.memoryTotal
            )}
          />
        </StatsRow>
        <StatsRow
          icon={ArrowDownIcon}
          label="Download"
          value={rate(latest.received)}
        >
          <StatsBars
            shares={shown.map((sample) => sample.received / receivedPeak)}
            calm
          />
        </StatsRow>
        <StatsRow icon={ArrowUpIcon} label="Upload" value={rate(latest.sent)}>
          <StatsBars
            shares={shown.map((sample) => sample.sent / sentPeak)}
            calm
          />
        </StatsRow>
        <StatsRow
          icon={HardDriveIcon}
          label="Disk"
          detail={bytes(machine.diskTotal)}
          used={bytes(machine.diskUsed)}
          value={percent(diskShare * 100)}
          meter
        >
          <div className="h-[3px] overflow-hidden rounded-full bg-popover-accent">
            <div
              className={`h-full rounded-full ${PILL_FILL[toneOf(diskShare)]}`}
              style={{ width: `${diskShare * 100}%` }}
            />
          </div>
        </StatsRow>
      </div>
    </div>
  )
}

function StatsRow({
  icon: Icon,
  label,
  detail,
  used,
  value,
  meter = false,
  children,
}: {
  icon: LucideIcon
  label: string
  detail?: string
  used?: string
  value: string
  meter?: boolean
  children: ReactNode
}) {
  return (
    <div className={`flex flex-col ${meter ? "gap-2" : "gap-1"}`}>
      <div className="flex items-center text-(length:--type-sm) leading-4 whitespace-nowrap tabular-nums">
        <Icon className="mr-[5px] size-3.5 shrink-0 text-muted-foreground" />
        <span className="text-muted-foreground">
          {label}
          {detail && (
            <>
              <Separator />
              {detail}
            </>
          )}
        </span>
        <span className="ml-auto pl-2 font-medium">
          {used && (
            <>
              <span className="text-muted-foreground">{used}</span>
              <Separator />
            </>
          )}
          <span className="text-foreground">{value}</span>
        </span>
      </div>
      {children}
    </div>
  )
}

function Separator() {
  return <span className="text-muted-stronger-foreground">{" · "}</span>
}

/** A bar for each sample, standing on a line of dots: one under every place a bar can stand. */
function StatsBars({
  shares,
  calm = false,
}: {
  shares: number[]
  calm?: boolean
}) {
  const pitch = CHART_WIDTH / shares.length
  const width = Math.max(1, pitch - 1)
  return (
    <svg
      viewBox={`0 0 ${CHART_WIDTH} ${CHART_HEIGHT}`}
      className="h-3 w-full"
      preserveAspectRatio="none"
      aria-hidden="true"
    >
      {shares.map((raw, index) => {
        const share = Math.min(Math.max(raw, 0), 1)
        const height =
          share > 0 ? Math.max(1, Math.round(share * CHART_HEIGHT)) : 0
        const x = index * pitch
        if (height === 0)
          return (
            <rect
              key={index}
              x={x}
              y={CHART_HEIGHT - 1}
              width={width}
              height={1}
              className="fill-muted-strongest-foreground"
            />
          )
        return (
          <rect
            key={index}
            x={x}
            y={CHART_HEIGHT - height}
            width={width}
            height={height}
            className={BAR_FILL[toneOf(share, calm)]}
          />
        )
      })}
    </svg>
  )
}
