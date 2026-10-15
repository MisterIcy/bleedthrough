<?php

class User
{
    public int $id;
    protected ?string $name = null;
    private static array $cache = [];
    public iterable $items = [];
}
