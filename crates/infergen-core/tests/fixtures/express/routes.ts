import { Router, Request, Response } from 'express';

const router = Router();

router.get('/products', async (req: Request, res: Response) => {
  const products = await ProductService.findAll();
  res.json(products);
});

router.post('/products', async (req: Request, res: Response) => {
  const product = await ProductService.create(req.body);
  res.status(201).json(product);
});

router.get('/products/:id', async (req: Request, res: Response) => {
  const product = await ProductService.findById(req.params.id);
  if (!product) return res.status(404).json({ error: 'not found' });
  res.json(product);
});

router.delete('/products/:id', async (req: Request, res: Response) => {
  await ProductService.delete(req.params.id);
  res.status(204).end();
});

router.post('/auth/login', async (req: Request, res: Response) => {
  const { email, password } = req.body;
  const token = await AuthService.login(email, password);
  res.json({ token });
});

export default router;
