<?php

function handle(string $value): void
{
    readfile('/var/www/files/' . basename($value));
}
