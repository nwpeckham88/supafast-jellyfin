import { afterEach, describe, expect, it, vi } from 'vitest';

import { generateUuid } from '$lib/utils/idempotency';

describe('generateUuid', () => {
  afterEach(() => {
    vi.unstubAllGlobals();
    vi.restoreAllMocks();
  });

  it('returns an RFC4122-like v4 UUID string', () => {
    const value = generateUuid();
    expect(value).toMatch(/^[0-9a-f]{8}-[0-9a-f]{4}-4[0-9a-f]{3}-[89ab][0-9a-f]{3}-[0-9a-f]{12}$/i);
  });

  it('generates unique values across calls', () => {
    const first = generateUuid();
    const second = generateUuid();
    expect(first).not.toBe(second);
  });

  it('uses getRandomValues fallback when randomUUID is unavailable', () => {
    const mockGetRandomValues = vi.fn((buffer: Uint8Array) => {
      for (let index = 0; index < buffer.length; index += 1) {
        buffer[index] = index;
      }

      return buffer;
    });

    vi.stubGlobal('crypto', {
      getRandomValues: mockGetRandomValues
    } as unknown as Crypto);

    const value = generateUuid();
    expect(mockGetRandomValues).toHaveBeenCalledOnce();
    expect(value).toMatch(/^[0-9a-f]{8}-[0-9a-f]{4}-4[0-9a-f]{3}-[89ab][0-9a-f]{3}-[0-9a-f]{12}$/i);
  });

  it('uses Math.random fallback when crypto is unavailable', () => {
    vi.stubGlobal('crypto', undefined);
    vi.spyOn(Math, 'random').mockReturnValue(0.25);

    const value = generateUuid();
    expect(value).toMatch(/^[0-9a-f]{8}-[0-9a-f]{4}-4[0-9a-f]{3}-[89ab][0-9a-f]{3}-[0-9a-f]{12}$/i);
  });
});
