<?php

enum Suit
{
    case Hearts;
    case Spades;

    const Wild = self::Spades;

    public function label(): string
    {
        return ucfirst($this->name);
    }

    public static function fromChar(string $c): self
    {
        return self::Hearts;
    }
}

enum Status: string implements HasLabel
{
    use LabelTrait;

    case Active = 'active';
    case Archived = 'archived';
}
