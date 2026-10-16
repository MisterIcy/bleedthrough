<?php

final class Handler
{
    public function handle(string $value): void
    {
        $object = unserialize($value, ['allowed_classes' => false]);
    }
}

(new Handler())->handle($_GET['x']);
