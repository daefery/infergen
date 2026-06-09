import { GetServerSideProps } from 'next';
import { getSession } from 'next-auth/react';

interface Props {
  user: { id: string; email: string } | null;
}

export default function HomePage({ user }: Props) {
  const handleSubscribe = async (e: React.FormEvent) => {
    e.preventDefault();
    const target = e.target as typeof e.target & { email: { value: string } };
    await fetch('/api/subscribe', {
      method: 'POST',
      body: JSON.stringify({ email: target.email.value }),
    });
  };

  return (
    <main>
      <h1>Welcome{user ? `, ${user.email}` : ''}</h1>
      <form onSubmit={handleSubscribe}>
        <input name="email" type="email" required />
        <button type="submit">Subscribe</button>
      </form>
    </main>
  );
}

export const getServerSideProps: GetServerSideProps = async (ctx) => {
  const session = await getSession(ctx);
  return { props: { user: session?.user ?? null } };
};
