<?php

declare(strict_types=1);

namespace App\Persistence;

use PDO;
use Generator;

interface Repository
{
    public function find(int $id): ?array;

    /** @return Generator<int, array> */
    public function all(): Generator;
}

abstract class PdoRepository implements Repository
{
    protected static int $queries = 0;

    public function __construct(protected PDO $pdo, protected string $table)
    {
    }

    public function find(int $id): ?array
    {
        $stmt = $this->pdo->prepare("SELECT * FROM {$this->table} WHERE id = :id");
        $stmt->execute(['id' => $id]);
        static::$queries++;

        $row = $stmt->fetch(PDO::FETCH_ASSOC);

        return $row === false ? null : $row;
    }

    public function all(): Generator
    {
        foreach ($this->pdo->query("SELECT * FROM {$this->table}") as $row) {
            yield (int) $row['id'] => $row;
        }
    }

    abstract protected function hydrate(array $row): object;
}

final class UserRepository extends PdoRepository
{
    public function __construct(PDO $pdo)
    {
        parent::__construct($pdo, 'users');
    }

    protected function hydrate(array $row): object
    {
        [
            'id' => $id,
            'name' => $name,
        ] = $row;

        return new class ($id, $name) {
            public function __construct(public int $id, public string $name)
            {
            }
        };
    }

    public function names(): array
    {
        $names = [];
        foreach ($this->all() as $id => ['name' => $name]) {
            $names[$id] = $name;
        }

        return $names;
    }
}
