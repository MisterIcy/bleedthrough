<?php
namespace App;

interface UserRepository
{
    public function find(int $id): ?User;
    /** @return list<User> */
    public function all(): array;
}

class User
{
    public function __construct(public int $id, private string $name) {}
    public function name(): string { return $this->name; }
}

class SqlUserRepository implements UserRepository
{
    public function find(int $id): ?User { return null; }
    public function all(): array { return []; }
}

class CachedUserRepository implements UserRepository
{
    public function find(int $id): ?User { return null; }
    public function all(): array { return []; }
}

class Response
{
    public static function json(array $data): static { return new static(); }
}

class UserController
{
    public function __construct(private UserRepository $users) {}

    public function show(int $id): Response
    {
        $user = $this->users->find($id);
        if ($user === null) {
            return Response::json(['error' => 'missing']);
        }
        $name = $user->name();
        return Response::json(['name' => $name, 'id' => $user->id]);
    }

    public function index(): Response
    {
        $out = [];
        foreach ($this->users->all() as $user) {
            $out[] = $user->name();
        }
        return Response::json($out);
    }

    public function dynamic($handler, $method)
    {
        $handler->handle();
        $this->$method();
        $this->missing();
        return strtoupper('x');
    }
}
