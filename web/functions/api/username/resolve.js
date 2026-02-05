// GET /api/username/resolve?uid=xxx — uid로 username 조회
// GET /api/username/resolve?name=xxx — username으로 uid 조회
export async function onRequestGet(context) {
  const { env, request } = context;
  const url = new URL(request.url);
  const uid = url.searchParams.get('uid');
  const name = url.searchParams.get('name');

  if (uid) {
    const obj = await env.VAULT.get(`_users/${uid}`);
    if (!obj) {
      return Response.json({ found: false });
    }
    const profile = JSON.parse(await obj.text());
    return Response.json({ found: true, ...profile });
  }

  if (name) {
    const obj = await env.VAULT.get(`_usernames/${name.toLowerCase()}`);
    if (!obj) {
      return Response.json({ found: false });
    }
    const data = JSON.parse(await obj.text());
    return Response.json({ found: true, ...data });
  }

  return Response.json({ error: 'uid or name required' }, { status: 400 });
}
