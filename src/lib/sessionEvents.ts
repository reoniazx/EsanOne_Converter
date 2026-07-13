export interface SessionEvent {
  sessionId?: string | null;
}

export function isCurrentSessionEvent(
  activeSessionId: string | null | undefined,
  event: SessionEvent
): boolean {
  return Boolean(activeSessionId && event.sessionId === activeSessionId);
}
