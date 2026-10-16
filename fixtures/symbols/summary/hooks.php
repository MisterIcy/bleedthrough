<?php
class Temperature
{
    public float $celsius {
        get => $this->celsius;
        set(float $value) { $this->celsius = $value; }
    }

    public function __construct(
        public private(set) int $reading {
            get => $this->reading * 2;
        },
    ) {}
}
