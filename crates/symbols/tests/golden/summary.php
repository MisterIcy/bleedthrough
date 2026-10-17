namespace App\Model;

use Lib\Base;
use Lib\Contracts\Jsonable;
use Lib\Contracts\Arrayable as Arr;
use function Lib\helper;
use const Lib\LIMIT;

const TOP = -5;
const FLOAT = 1.5;
const TRUE_CONST = true;
const PAREN = (7);
const BIG = 99999999999999999999;
const LIST = [1, 'a' => 2];
const STR = 'plain';
const ESC = 'it\'s';
const DQ = "dq";
const NIL = null;
const T2 = TRUE;
const F2 = FALSE;
const N2 = NULL;
const NEGF = -1.5;
const NEGX = -FOO;
const OCT = 010;
const HEX = 0x1F;
const BIN = 0b101;
const NEWOCT = 0o17;
const UND = 1_000;
const ZERO = 0;
const DQE = "a\n";
const FQ = \Lib\Other;
define('DEFINED', 'x');
define('Lib\\NS_DEFINED', 3);
define($dynamic, 1);
if (true) { function cond() {} }
declare(strict_types=1) { function declared() {} }
{ function blocky() {} }

/**
 * @template T of object
 * @extends Base<T>
 * @implements Arr<string, int>
 * @property int $magic
 * @property-read string $ro
 * @property-write bool $wo
 * @method static self make(int $a, string ...$rest)
 * @method string name()
 * @mixin Other
 */
#[Entity, Table('t')]
final class Item extends Base implements Jsonable, Arr {
    use Loggable, Cacheable { Loggable::log insteadof Cacheable; Cacheable::log as protected cacheLog; log as public; }
    const A = 1;
    final public const B = 'b';
    protected const C = self::A;
    public int $id = 0;
    public ?string $name = null;
    protected static array $registry = [];
    private readonly float $ratio;
    public private(set) int $guarded = 1;
    public protected(set) int $semi = 2;
    public public(set) int $open = 3;
    /** @var list<int> */
    public array $ids = [];
    /** @var int $named */
    public $named, $other;
    public function __construct(public int $x = 0, protected readonly ?Base $base = null, string $plain = 'p', private $untyped = null) {}
    /**
     * @param T $value
     * @return T
     */
    public function pass($value) { return $value; }
    public static function build(int &$ref, string ...$rest): static { return new static(); }
    abstract protected function nothing(): void;
    final private function secret(?int $a = null, int|string $u = 1): ?int { return null; }
    public function nullDefault(int $a = null, int $b = NULL, int $c = (null), int $d = 0) {}
}

abstract class Shape { abstract function area(): float; }
readonly class Frozen {}
interface Jsonable2 extends Jsonable, Arr { const I = 1; function go(): void; }
trait Loggable { public $log; function log() {} abstract function need(); static function stat() {} }
enum Suit: string implements Jsonable { case H = 'h'; case S = 's'; const D = self::H; function label(): string { return 'x'; } }
enum Plain { case One; case Two; }

function helper2(int $a, ...$rest): void {}
/** @param list<int> $xs @return int */
function typed($xs) { return 1; }
$anon = new class extends Base {};
function & byRefReturn() {}

class Promo {
    function __construct(readonly int $z, int $notprom, #[SensitiveParameter] private string $secret = '') {}
    function other(int $q = 1) {}
}
class Refs extends \Fully\Qualified implements namespace\Rel {
    function m(Typed $a, ?Nullable $b, A|B $c, self $d): Ret {
        $x = new Created;
        Stat::call();
        $y = $x instanceof Inst;
        try { } catch (Caught | Other $e) { }
        helper();
        \global_fn();
        Lib\sub\func();
        $k = CONSTANT . \Lib\FQCONST;
        $f = strlen(...);
        $g = new class extends Anon {};
        return Stat::$prop . Cls::KONST . parent::x() . static::y();
    }
}

class Odd { function bare(); }
interface Bodied { function with() {} function without(); }
