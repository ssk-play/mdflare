// POST /api/:userId/rename — 이름 변경
export async function onRequestPost(context) {
  const { params, env, request } = context;
  const userId = params.userId;
  const body = await request.json();
  const { oldPath, newPath } = body;

  const oldKey = `${userId}/${oldPath}`;
  const newKey = `${userId}/${newPath}`;

  try {
    const object = await env.VAULT.get(oldKey);
    if (!object) {
      return Response.json({ error: 'Not found' }, { status: 404 });
    }

    const content = await object.text();
    await env.VAULT.put(newKey, content, {
      customMetadata: object.customMetadata
    });
    await env.VAULT.delete(oldKey);

    return Response.json({ renamed: true, oldPath, newPath });
  } catch (err) {
    return Response.json({ error: err.message }, { status: 500 });
  }
}
