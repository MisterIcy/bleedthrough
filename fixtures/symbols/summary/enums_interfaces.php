<?php
namespace Shop {
    interface HasPrice extends \Stringable, \JsonSerializable
    {
        public const CURRENCY = 'EUR';
        public function price(): float;
    }

    enum Suit: string implements HasPrice
    {
        case Hearts = 'H';
        case Spades = 'S';
        const Wild = self::Spades;

        public function price(): float { return 1.0; }
        public static function fromChar(string $c): self { return self::from($c); }
    }

    enum Plain { case One; case Two; }

    trait Counts
    {
        private int $count = 0;
        abstract public function limit(): int;
        public function inc(): static { $this->count++; return $this; }
    }
}

namespace Other {
    use Shop\Suit;

    class Holder
    {
        public function __construct(public Suit $suit = Suit::Hearts) {}
    }
}
