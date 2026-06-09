import type { NextApiRequest, NextApiResponse } from 'next';
import { getSession } from 'next-auth/react';

export default async function handler(req: NextApiRequest, res: NextApiResponse) {
  const session = await getSession({ req });
  if (!session) {
    return res.status(401).json({ error: 'unauthorized' });
  }
  if (req.method === 'GET') {
    const users = [{ id: '1', name: 'Alice' }];
    return res.status(200).json(users);
  }
  res.status(405).end();
}
