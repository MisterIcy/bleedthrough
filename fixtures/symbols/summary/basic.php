<?php
namespace App\Model;

use App\Contracts\Entity;
use App\Support\{Arr, Str as S};
use function App\Support\helper;

const VERSION = '1.0';
define('App\Model\LEGACY', 3);

/**
 * @template T
 * @property-read int $id
 * @method static self make(array<string, mixed> $attrs)
 */
#[\Attribute]
final class User extends Base implements Entity, \Countable
{
    use Greets, Logs {
        Greets::hello insteadof Logs;
        Logs::hello as protected logHello;
    }

    public const ROLE = 'user';
    private const int LIMIT = -5;

    /** @var list<string> */
    protected array $tags = [];
    public static ?Arr $shared = null;

    public function __construct(
        private readonly int $age,
        /** @var positive-int */
        public int|string $key = 1,
        protected ?S $name = null,
    ) {
    }

    /**
     * @param array<string, int> $map
     * @return T|null
     */
    public function find(array $map, int ...$rest): mixed
    {
        helper($map);
        return strlen('x') > 0 ? new Child() : null;
    }

    abstract protected function hidden(): static;
    public function count(): int { return 0; }
}

function top(self|int $x, $untyped = null): void {}
