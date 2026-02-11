// PUT /api/:userId/agent-status — 에이전트 heartbeat 기록
export async function onRequestPut(context) {
  const { env, data } = context;

  if (!data.isOwner) {
    return Response.json({ error: 'Unauthorized' }, { status: 403 });
  }

  const uid = data.resolvedUid;
  const now = new Date().toISOString();

  await env.VAULT.put(`_meta/${uid}/agent_heartbeat`, now);

  return Response.json({ ok: true, lastSync: now });
}

// GET /api/:userId/agent-status — 에이전트 상태 조회
export async function onRequestGet(context) {
  const { env, data } = context;

  if (!data.isOwner) {
    return Response.json({ error: 'Unauthorized' }, { status: 403 });
  }

  const uid = data.resolvedUid;
  const obj = await env.VAULT.get(`_meta/${uid}/agent_heartbeat`);

  if (!obj) {
    return Response.json({ connected: false, lastSync: null, minutesAgo: null });
  }

  const lastSync = await obj.text();
  const minutesAgo = Math.floor((Date.now() - new Date(lastSync).getTime()) / 60000);
  const connected = minutesAgo < 5;

  return Response.json({ connected, lastSync, minutesAgo });
}
