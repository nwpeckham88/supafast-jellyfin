import type { ApiEnvelope } from '$lib/models/api';

const API_BASE_URL = import.meta.env.PUBLIC_API_BASE_URL ?? '';

export class ApiClientError extends Error {
  readonly code: string;
  readonly status: number;

  constructor(code: string, message: string, status: number) {
    super(message);
    this.code = code;
    this.status = status;
  }
}

function buildUrl(path: string): string {
  if (!API_BASE_URL) {
    return path;
  }
  return `${API_BASE_URL}${path}`;
}

export async function request<T>(
  path: string,
  init: RequestInit = {}
): Promise<T> {
  const method = (init.method ?? 'GET').toUpperCase();
  const canRetry = method === 'GET' || method === 'HEAD' || method === 'OPTIONS';

  const executeRequest = async (): Promise<T> => {
    const response = await fetch(buildUrl(path), {
      headers: {
        'content-type': 'application/json',
        ...(init.headers ?? {})
      },
      ...init
    });

    let envelope: ApiEnvelope<T>;
    try {
      envelope = (await response.json()) as ApiEnvelope<T>;
    } catch {
      throw new ApiClientError(
        'invalid_response',
        'server returned a non-json response',
        response.status
      );
    }

    if (!response.ok || !envelope.success || envelope.data == null) {
      const code = envelope.error?.code ?? 'request_failed';
      const message = envelope.error?.message ?? 'request failed';
      throw new ApiClientError(code, message, response.status);
    }

    return envelope.data;
  };

  try {
    return await executeRequest();
  } catch (error) {
    const transientHttpError = error instanceof ApiClientError && error.status >= 500;
    if (!transientHttpError || !canRetry) {
      throw error;
    }

    return executeRequest();
  }
}