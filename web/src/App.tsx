import { ArrowUpRight, GithubLogo } from '@phosphor-icons/react'
import type { CSSProperties } from 'react'
import { Bezel } from './components/Bezel'
import { SplitDemo } from './components/SplitDemo'

export const REPO_URL = 'https://github.com/gwanryo/retouch'
export const SPEC_URL = `${REPO_URL}/blob/main/.omc/specs/deep-interview-photo-edit-match-game.md`
const PROPOSE_URL = `${REPO_URL}/issues/new?template=challenge_proposal.yml`
const DEMO_SRC = `${import.meta.env.BASE_URL}demo/sunset-800.jpg`

const delay = (ms: number) => ({ '--d': `${ms}ms` }) as CSSProperties

const STEPS = [
  {
    n: '01',
    title: '원본과 정답을 본다',
    body: '한 장의 사진과 그 사진을 보정한 정답이 나란히 놓입니다. 정답은 우리 엔진으로 렌더한 것이라 항상 도달할 수 있습니다.',
  },
  {
    n: '02',
    title: '에디터로 따라 보정한다',
    body: 'Lightroom 호환 슬라이더, HSL, 톤커브, 컬러 그레이딩, 선형·방사형·범위 마스크. 슬라이더 값이 아니라 결과가 정답에 가까우면 됩니다.',
  },
  {
    n: '03',
    title: '점수와 해설을 받는다',
    body: '지각 색차 ΔE00로 0~100점. 톤, 색, 디테일, 로컬, 구도 다섯 지표와 정답 레시피 해설로 무엇이 달랐는지 배웁니다.',
  },
]

const PRINCIPLES = [
  ['정답은 항상 도달 가능', '정답 레시피를 그대로 넣으면 어디서나 100점. 기기가 달라도 같은 픽셀이 나옵니다.'],
  ['값이 아니라 결과를 채점', '다른 슬라이더 조합으로 같은 결과를 내도 만점. 채점은 눈이 보는 것을 봅니다.'],
  ['서버도 계정도 없음', '정적 사이트 하나. 렌더, 채점, 공유 카드 생성이 전부 브라우저 안에서 끝납니다.'],
]

export default function App() {
  return (
    <div className="min-h-[100dvh]">
      <a
        href="#main"
        className="sr-only focus:not-sr-only focus:fixed focus:top-3 focus:left-3 focus:z-30 focus:rounded-full focus:bg-accent focus:px-4 focus:py-2 focus:text-accent-ink"
      >
        본문으로 건너뛰기
      </a>

      {/* Floating island nav, detached from the top edge */}
      <header className="rise sticky top-4 z-20 mx-auto mt-4 flex w-max items-center gap-1 rounded-full bg-card/80 p-1.5 pl-4 ring-1 ring-fg/[0.08] backdrop-blur-xl" style={delay(0)}>
        <a href="#main" className="font-display text-[17px] font-semibold tracking-[-0.02em]">
          Retouch
        </a>
        <span className="ml-3 hidden rounded-full bg-fg/[0.05] px-3 py-1.5 font-mono text-[11px] tracking-[0.08em] text-fg-2 sm:inline-block">
          준비 중
        </span>
        <a
          href={REPO_URL}
          aria-label="GitHub 저장소"
          className="ml-1 grid size-9 place-items-center rounded-full text-fg-2 transition-[background-color,color] duration-500 ease-[var(--ease-out-expo)] hover:bg-fg/[0.06] hover:text-fg"
        >
          <GithubLogo size={18} weight="regular" />
        </a>
      </header>

      <main id="main" className="mx-auto w-full max-w-[1200px] scroll-mt-24 px-4 sm:px-8">
        {/* ---------------------------------------------------------- Hero */}
        <section className="grid items-center gap-12 pt-16 pb-24 sm:pt-24 lg:grid-cols-12 lg:gap-10 lg:pb-32">
          <div className="lg:col-span-6">
            <p className="rise inline-flex rounded-full bg-accent/10 px-3 py-1 font-mono text-[10px] font-medium tracking-[0.2em] text-accent uppercase" style={delay(80)}>
              Photo retouch training game
            </p>
            <h1
              className="rise mt-6 font-display text-[clamp(2rem,3.4vw,3rem)] leading-[1.12] font-bold tracking-[-0.03em] break-keep"
              style={delay(160)}
            >
              정답 사진을 보고,
              <br />
              같은 보정을 만들어내세요.
            </h1>
            <p className="rise mt-6 max-w-[34ch] text-[17px] leading-relaxed text-fg-2" style={delay(240)}>
              원본과 정답을 비교하며 에디터로 재현하고, 지각 색차로 채점받고, 정답 레시피로 배웁니다.
            </p>
            <div className="rise mt-9 flex flex-wrap items-center gap-3" style={delay(320)}>
              <a
                href={SPEC_URL}
                className="group inline-flex h-12 items-center gap-3 rounded-full bg-fg pr-1.5 pl-6 text-[15px] font-medium text-bg transition-transform duration-500 ease-[var(--ease-spring)] hover:-translate-y-0.5 active:scale-[0.98]"
              >
                스펙 읽기
                <span className="grid size-9 place-items-center rounded-full bg-bg/15 transition-transform duration-500 ease-[var(--ease-spring)] group-hover:translate-x-0.5 group-hover:-translate-y-px group-hover:scale-105">
                  <ArrowUpRight size={16} weight="regular" />
                </span>
              </a>
              <a
                href={PROPOSE_URL}
                className="inline-flex h-12 items-center rounded-full px-6 text-[15px] font-medium text-fg-2 ring-1 ring-fg/[0.12] transition-[color,box-shadow] duration-500 ease-[var(--ease-out-expo)] hover:text-fg hover:ring-fg/30"
              >
                챌린지 제안
              </a>
            </div>
          </div>

          <div className="rise lg:col-span-6" style={delay(200)}>
            <SplitDemo />
          </div>
        </section>

        {/* -------------------------------------------- How it plays (bento) */}
        <section aria-labelledby="how" className="scroll-mt-24 py-24 lg:py-32">
          <h2 id="how" className="font-display text-[clamp(1.7rem,3.4vw,2.4rem)] font-bold tracking-[-0.02em]">
            플레이 방법
          </h2>

          <ol className="mt-10 grid gap-4 lg:grid-cols-12 lg:grid-rows-2">
            {/* Cell A: step 01 with the real photo */}
            <li className="rise lg:col-span-7 lg:row-span-2" style={delay(100)}>
              <Bezel className="h-full transition-transform duration-700 ease-[var(--ease-spring)] hover:-translate-y-1" inner="flex h-full flex-col">
                <img
                  src={DEMO_SRC}
                  alt=""
                  width={800}
                  height={600}
                  loading="lazy"
                  decoding="async"
                  className="aspect-[16/9] w-full object-cover object-[50%_65%] [filter:saturate(.7)_contrast(.9)] lg:aspect-auto lg:flex-1"
                />
                <div className="p-6 sm:p-8">
                  <StepHead n={STEPS[0].n} title={STEPS[0].title} />
                  <p className="mt-3 max-w-[52ch] text-[15px] leading-relaxed text-fg-2">{STEPS[0].body}</p>
                </div>
              </Bezel>
            </li>
            {/* Cell B: tinted */}
            <li className="rise lg:col-span-5" style={delay(190)}>
              <Bezel className="h-full transition-transform duration-700 ease-[var(--ease-spring)] hover:-translate-y-1" inner="h-full bg-accent/[0.08] p-6 sm:p-8">
                <StepHead n={STEPS[1].n} title={STEPS[1].title} />
                <p className="mt-3 text-[15px] leading-relaxed text-fg-2">{STEPS[1].body}</p>
              </Bezel>
            </li>
            {/* Cell C: plain */}
            <li className="rise lg:col-span-5" style={delay(280)}>
              <Bezel className="h-full transition-transform duration-700 ease-[var(--ease-spring)] hover:-translate-y-1" inner="h-full p-6 sm:p-8">
                <StepHead n={STEPS[2].n} title={STEPS[2].title} />
                <p className="mt-3 text-[15px] leading-relaxed text-fg-2">{STEPS[2].body}</p>
              </Bezel>
            </li>
          </ol>
        </section>

        {/* ----------------------------------------------- Share (stack + card) */}
        <section aria-labelledby="share" className="scroll-mt-24 py-24 lg:py-32">
          <div className="max-w-[60ch]">
            <h2 id="share" className="font-display text-[clamp(1.7rem,3.4vw,2.4rem)] font-bold tracking-[-0.02em]">
              스포일러 없는 공유
            </h2>
            <p className="mt-5 text-[17px] leading-relaxed text-fg-2">
              카드에는 점수와 <strong className="font-medium text-fg">내 결과</strong>만 들어갑니다. 정답도 원본도 보이지 않으니, 받은 친구는 힌트 없이 같은 챌린지에 도전할 수 있습니다.
            </p>
            <p className="mt-4 flex flex-wrap gap-x-4 gap-y-1 font-mono text-[13px] leading-relaxed text-muted" aria-label="공유 텍스트 예시">
              <span className="w-full">Retouch #012 87점</span>
              <span className="whitespace-nowrap">🟩🟩🟩🟨⬜ 톤</span>
              <span className="whitespace-nowrap">🟩🟩🟩🟩🟨 색</span>
              <span className="whitespace-nowrap">🟩🟩🟨⬜⬜ 디테일</span>
            </p>
          </div>

          <div className="rise mt-12 w-full max-w-[440px] md:ml-auto md:-mt-24 md:rotate-[-2deg]" style={delay(140)}>
            <Bezel className="shadow-[var(--shadow-soft)]" inner="p-5 sm:p-6">
              <div className="flex items-baseline justify-between">
                <span className="font-display text-lg font-semibold tracking-[-0.02em]">Retouch</span>
                <span className="font-mono text-[11px] tracking-[0.08em] text-muted">#012 필름 룩</span>
              </div>
              <div className="mt-4 aspect-[3/2] overflow-hidden rounded-2xl">
                <img src={DEMO_SRC} alt="" width={800} height={600} loading="lazy" decoding="async" className="size-full object-cover [filter:saturate(1.2)_contrast(1.1)]" />
              </div>
              <div className="mt-5 flex items-end justify-between gap-6">
                <div>
                  <p className="font-mono text-[11px] tracking-[0.08em] text-muted">보정 점수</p>
                  <p className="font-display text-[64px] leading-none font-light tracking-[-0.04em] text-accent">87</p>
                </div>
                <dl className="grid grid-cols-[auto_1fr] items-center gap-x-3 gap-y-1.5 font-mono text-[12px]">
                  <Bar label="톤" v={4} />
                  <Bar label="색" v={5} />
                  <Bar label="디테일" v={3} />
                </dl>
              </div>
            </Bezel>
          </div>
        </section>

        {/* ---------------------------------------------- Principles (list) */}
        <section aria-labelledby="principles" className="grid scroll-mt-24 gap-10 py-24 lg:grid-cols-12 lg:py-32">
          <h2 id="principles" className="font-display text-[clamp(1.7rem,3.4vw,2.4rem)] font-bold tracking-[-0.02em] lg:col-span-4">
            원칙
          </h2>
          <dl className="divide-y divide-line lg:col-span-8">
            {PRINCIPLES.map(([t, b], i) => (
              <div key={t} className="rise grid gap-2 py-6 first:pt-0 last:pb-0 sm:grid-cols-[14rem_1fr] sm:gap-8" style={delay(80 + i * 80)}>
                <dt className="text-[17px] font-semibold tracking-[-0.01em]">{t}</dt>
                <dd className="m-0 text-[15px] leading-relaxed text-fg-2">{b}</dd>
              </div>
            ))}
          </dl>
        </section>
      </main>

      <footer className="mx-auto w-full max-w-[1200px] px-4 pt-6 pb-12 sm:px-8">
        <div className="h-px bg-line" />
        <div className="mt-6 flex flex-col gap-3 font-mono text-[12px] text-muted sm:flex-row sm:items-center sm:justify-between">
          <p>© 2026 Ryo. 코드는 MIT, 챌린지 사진은 CC0/Unsplash/Pexels 출처 표기.</p>
          <p>
            데모 사진: Jake Givens (Unsplash)
          </p>
        </div>
      </footer>
    </div>
  )
}

function StepHead({ n, title }: { n: string; title: string }) {
  return (
    <div className="flex items-baseline gap-4">
      <span className="font-display text-[15px] font-medium tracking-[0.02em] text-accent">{n}</span>
      <h3 className="text-xl font-semibold tracking-[-0.01em]">{title}</h3>
    </div>
  )
}

function Bar({ label, v }: { label: string; v: number }) {
  return (
    <>
      <dt className="text-muted">{label}</dt>
      <dd className="m-0 flex gap-[3px]" aria-label={`${v} / 5`}>
        {Array.from({ length: 5 }).map((_, i) => (
          <span key={i} className={`h-3 w-4 rounded-[3px] ${i < v ? 'bg-accent' : 'bg-fg/[0.08]'}`} />
        ))}
      </dd>
    </>
  )
}
