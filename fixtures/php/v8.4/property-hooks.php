<?php

class User
{
    public string $fullName {
        get => $this->first . ' ' . $this->last;
        set(string $value) {
            [$this->first, $this->last] = explode(' ', $value, 2);
        }
    }

    public int $age {
        set {
            $this->age = $value;
        }
    }

    public function __construct(public string $first = '', public string $last = '') {}
}

interface HasName
{
    public string $name { get; }
}

abstract class Base
{
    abstract public string $title { get; set; }
    public string $both { &get => $this->both; }
}
