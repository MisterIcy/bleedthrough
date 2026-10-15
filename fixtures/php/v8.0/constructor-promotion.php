<?php

class Point
{
    public function __construct(
        public int $x = 0,
        protected ?int $y = null,
        private string $label = 'p',
    ) {}
}
