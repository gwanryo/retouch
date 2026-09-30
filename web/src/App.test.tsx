import { fireEvent, render, screen } from '@testing-library/react'
import { describe, expect, it } from 'vitest'
import App, { REPO_URL, SPEC_URL } from './App'

describe('landing page', () => {
  it('renders the two-line headline and the primary spec CTA', () => {
    render(<App />)
    // The headline is split across two lines with <br />, so allow no space at the break.
    expect(screen.getByRole('heading', { level: 1 })).toHaveTextContent(/정답 사진을 보고,\s*같은 보정을 만들어내세요\./)
    expect(screen.getByRole('link', { name: /스펙 읽기/ })).toHaveAttribute('href', SPEC_URL)
    expect(screen.getByRole('link', { name: 'GitHub 저장소' })).toHaveAttribute('href', REPO_URL)
  })

  it('lets the visitor move the before/after split', () => {
    render(<App />)
    const range = screen.getByRole('slider', { name: '원본과 정답 비교 위치' })
    fireEvent.change(range, { target: { value: '70' } })
    expect(range).toHaveValue('70')
    expect(range).toHaveAttribute('aria-valuetext', '정답 70% 표시')
  })

  it('lists exactly three play steps in order', () => {
    render(<App />)
    const items = screen.getAllByRole('listitem')
    expect(items).toHaveLength(3)
    expect(items.map((li) => li.textContent)).toEqual([
      expect.stringContaining('원본과 정답을 본다'),
      expect.stringContaining('에디터로 따라 보정한다'),
      expect.stringContaining('점수와 해설을 받는다'),
    ])
  })

  it('has no em-dashes in visible copy', () => {
    const { container } = render(<App />)
    expect(container.textContent).not.toMatch(/[—–]/)
  })
})
