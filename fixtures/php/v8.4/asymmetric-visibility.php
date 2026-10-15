<?php

class Account
{
    public private(set) string $owner = '';
    public protected(set) int $balance = 0;
    protected private(set) ?string $note = null;
    public function __construct(public private(set) string $id = '') {}
}
